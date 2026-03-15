use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use syn::{
    File, Item, ItemConst, ItemEnum, ItemFn, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemTraitAlias, ItemType, ItemUnion, ItemUse, UseTree, Visibility, spanned::Spanned,
};

use super::{Diagnostic, DiagnosticLevel, NamespaceSettings};

pub(super) struct NamespaceAnalysis {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone)]
struct UseLeaf {
    full_path: Vec<String>,
    source_name: Option<String>,
    binding_name: Option<String>,
    kind: UseLeafKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UseLeafKind {
    Name,
    Rename,
    Glob,
}

pub(super) fn analyze_namespace_rules(
    path: &Path,
    parsed: &File,
    settings: &NamespaceSettings,
) -> NamespaceAnalysis {
    let mut diagnostics = Vec::new();
    analyze_scope(path, &parsed.items, settings, &mut diagnostics);
    NamespaceAnalysis { diagnostics }
}

fn analyze_scope(
    path: &Path,
    items: &[Item],
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for item in items {
        match item {
            Item::Use(item_use) => analyze_use_item(path, item_use, settings, diagnostics),
            Item::Mod(ItemMod {
                content: Some((_, nested)),
                ..
            }) => analyze_scope(path, nested, settings, diagnostics),
            _ => {}
        }
    }
}

fn analyze_use_item(
    path: &Path,
    item_use: &ItemUse,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut leaves = Vec::new();
    flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
    let line = item_use.span().start().line;
    let is_reexport = !matches!(item_use.vis, Visibility::Inherited);

    for leaf in leaves {
        if matches!(leaf.kind, UseLeafKind::Glob) {
            continue;
        }
        let Some(source_name) = &leaf.source_name else {
            continue;
        };
        let binding_name = leaf.binding_name.as_deref().unwrap_or(source_name);
        if is_nonbinding_import(source_name) || is_nonbinding_import(binding_name) {
            continue;
        }

        let analysis_path = trim_relative_prefix(&leaf.full_path);
        let Some(parent_module) = analysis_path.iter().rev().nth(1).cloned() else {
            continue;
        };
        let parent_normalized = parent_module.to_ascii_lowercase();
        let current_module_path = inferred_file_module_path(path);
        let redundant_leaf =
            redundant_leaf_context_candidate(analysis_path, binding_name, leaf.kind, settings);
        let skip_reexport = is_reexport
            && ((redundant_leaf.is_some() && direct_child_module_is_private(path, analysis_path))
                || canonical_parent_surface_reexport(
                    &current_module_path,
                    analysis_path,
                    binding_name,
                    settings,
                )
                || parent_surface_reexports_current_binding(
                    path,
                    &current_module_path,
                    binding_name,
                )
                || preserved_parent_surface_reexport(
                    &current_module_path,
                    analysis_path,
                    settings,
                ));

        if skip_reexport {
            continue;
        }

        let (code, message) = if !is_reexport
            && let Some(canonical_parent_surface) = canonical_parent_surface_candidate(
                path,
                &current_module_path,
                analysis_path,
                binding_name,
                settings,
            ) {
            canonical_parent_surface_message(
                binding_name,
                source_name,
                &parent_module,
                &canonical_parent_surface,
            )
        } else if let Some(shorter_leaf) = redundant_leaf {
            redundant_context_message(is_reexport, &parent_module, binding_name, &shorter_leaf)
        } else if settings.generic_nouns.contains(binding_name) {
            generic_noun_message(is_reexport, &parent_module, source_name)
        } else if settings
            .namespace_preserving_modules
            .contains(&parent_normalized)
            && !module_path_contains_namespace(&current_module_path, &parent_normalized)
        {
            preserve_module_message(is_reexport, &parent_module, source_name, binding_name)
        } else {
            continue;
        };

        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some(code.to_string()),
            policy: true,
            message,
        });
    }
}

fn canonical_parent_surface_reexport(
    current_module_path: &[String],
    import_path: &[String],
    binding_name: &str,
    settings: &NamespaceSettings,
) -> bool {
    if import_path.len() < 2 {
        return false;
    }

    let import_modules = &import_path[..import_path.len() - 1];
    let Some(imported_parent) = import_modules.last() else {
        return false;
    };
    let imported_parent_normalized = imported_parent.to_ascii_lowercase();
    let binding_normalized = binding_name.to_ascii_lowercase();

    if settings
        .organizational_modules
        .contains(&imported_parent_normalized)
    {
        return true;
    }

    if imported_parent_normalized == binding_normalized {
        return true;
    }

    !current_module_path.is_empty()
        && import_modules.ends_with(current_module_path)
        && settings.generic_nouns.contains(binding_name)
}

fn parent_surface_reexports_current_binding(
    path: &Path,
    current_module_path: &[String],
    binding_name: &str,
) -> bool {
    if current_module_path.is_empty() {
        return false;
    }

    let parent_surface_path = &current_module_path[..current_module_path.len() - 1];
    let public_bindings = public_bindings_for_module(path, parent_surface_path);
    public_bindings.contains(binding_name)
}

fn preserved_parent_surface_reexport(
    current_module_path: &[String],
    import_path: &[String],
    settings: &NamespaceSettings,
) -> bool {
    if current_module_path.is_empty() || import_path.len() != 2 {
        return false;
    }

    let Some(current_module) = current_module_path.last() else {
        return false;
    };
    let Some(imported_parent) = import_path.first() else {
        return false;
    };

    settings
        .namespace_preserving_modules
        .contains(&current_module.to_ascii_lowercase())
        && settings
            .namespace_preserving_modules
            .contains(&imported_parent.to_ascii_lowercase())
}

fn flatten_use_tree(prefix: Vec<String>, tree: &UseTree, leaves: &mut Vec<UseLeaf>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            flatten_use_tree(next, &path.tree, leaves);
        }
        UseTree::Name(name) => {
            let mut full_path = prefix;
            let source_name = name.ident.to_string();
            full_path.push(source_name.clone());
            leaves.push(UseLeaf {
                full_path,
                source_name: Some(source_name.clone()),
                binding_name: Some(source_name),
                kind: UseLeafKind::Name,
            });
        }
        UseTree::Rename(rename) => {
            let mut full_path = prefix;
            full_path.push(rename.ident.to_string());
            leaves.push(UseLeaf {
                full_path,
                source_name: Some(rename.ident.to_string()),
                binding_name: Some(rename.rename.to_string()),
                kind: UseLeafKind::Rename,
            });
        }
        UseTree::Glob(_) => leaves.push(UseLeaf {
            full_path: prefix,
            source_name: None,
            binding_name: None,
            kind: UseLeafKind::Glob,
        }),
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(prefix.clone(), item, leaves);
            }
        }
    }
}

fn generic_noun_message(
    is_reexport: bool,
    parent_module: &str,
    source_name: &str,
) -> (&'static str, String) {
    if is_reexport {
        (
            "namespace_flat_pub_use",
            format!(
                "flattened re-export hides namespace for `{source_name}`; keep `{parent_module}::{source_name}` visible"
            ),
        )
    } else {
        (
            "namespace_flat_use",
            format!(
                "flattened import hides namespace for `{source_name}`; prefer `{parent_module}::{source_name}` at call sites"
            ),
        )
    }
}

fn redundant_context_message(
    is_reexport: bool,
    parent_module: &str,
    binding_name: &str,
    shorter_leaf: &str,
) -> (&'static str, String) {
    if is_reexport {
        (
            "namespace_flat_pub_use_redundant_leaf_context",
            format!(
                "flattened re-export keeps redundant context in `{binding_name}`; prefer `{parent_module}::{shorter_leaf}`"
            ),
        )
    } else {
        (
            "namespace_flat_use_redundant_leaf_context",
            format!(
                "flattened import keeps redundant context in `{binding_name}`; prefer `{parent_module}::{shorter_leaf}` and keep `{parent_module}` visible at call sites"
            ),
        )
    }
}

fn preserve_module_message(
    is_reexport: bool,
    parent_module: &str,
    source_name: &str,
    binding_name: &str,
) -> (&'static str, String) {
    if is_reexport {
        (
            "namespace_flat_pub_use_preserve_module",
            format!(
                "flattened re-export hides configured namespace module `{parent_module}` for `{source_name}`; keep `{parent_module}::{source_name}` visible"
            ),
        )
    } else {
        (
            "namespace_flat_use_preserve_module",
            format!(
                "flattened import hides configured namespace module `{parent_module}` for `{binding_name}`; prefer `{parent_module}::{source_name}` at call sites"
            ),
        )
    }
}

fn redundant_leaf_context_candidate(
    full_path: &[String],
    leaf_name: &str,
    kind: UseLeafKind,
    settings: &NamespaceSettings,
) -> Option<String> {
    let parent_module = full_path.iter().rev().nth(1)?;
    let module_segments = split_segments(parent_module)
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let leaf_segments = split_segments(leaf_name);
    if module_segments.is_empty() || leaf_segments.len() <= module_segments.len() {
        return None;
    }

    let leaf_normalized = leaf_segments
        .iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let style = detect_name_style(leaf_name);

    if leaf_normalized.starts_with(&module_segments) {
        let shorter_segments = &leaf_segments[module_segments.len()..];
        if !shorter_segments.is_empty() {
            let shorter_leaf = render_segments(shorter_segments, style);
            if prefix_overlap_is_actionable(full_path, kind, &shorter_leaf) {
                return Some(shorter_leaf);
            }
        }
    }

    if leaf_normalized.ends_with(&module_segments)
        && suffix_overlap_is_actionable(parent_module, full_path)
    {
        let shorter_segments = &leaf_segments[..leaf_segments.len() - module_segments.len()];
        if !shorter_segments.is_empty() {
            return Some(render_segments(shorter_segments, style));
        }
    }

    // Keep the older generic-noun backstop for shapes like `user::UserRepository`.
    let preserve_or_generic = settings
        .namespace_preserving_modules
        .contains(&parent_module.to_ascii_lowercase())
        || split_segments(leaf_name)
            .iter()
            .any(|segment| matches_generic_noun(segment, settings));

    if preserve_or_generic && leaf_normalized.starts_with(&module_segments) {
        let shorter_segments = &leaf_segments[module_segments.len()..];
        if !shorter_segments.is_empty() {
            return Some(render_segments(shorter_segments, style));
        }
    }

    None
}

fn prefix_overlap_is_actionable(
    full_path: &[String],
    kind: UseLeafKind,
    shorter_leaf: &str,
) -> bool {
    if is_unreadable_short_leaf(shorter_leaf) {
        return false;
    }

    matches!(kind, UseLeafKind::Rename) || full_path.len() <= 3
}

fn suffix_overlap_is_actionable(parent_module: &str, full_path: &[String]) -> bool {
    if full_path.len() > 3 {
        return false;
    }

    split_segments(parent_module)
        .last()
        .is_some_and(|segment| is_suffix_category(segment))
}

fn is_suffix_category(segment: &str) -> bool {
    matches!(
        segment.to_ascii_lowercase().as_str(),
        "config" | "state" | "content" | "kind" | "attr"
    )
}

fn is_unreadable_short_leaf(shorter_leaf: &str) -> bool {
    matches!(
        shorter_leaf.to_ascii_lowercase().as_str(),
        "buf" | "ref" | "into" | "from" | "system"
    )
}

fn matches_generic_noun(segment: &str, settings: &NamespaceSettings) -> bool {
    settings
        .generic_nouns
        .iter()
        .any(|noun| noun.eq_ignore_ascii_case(segment))
}

fn trim_relative_prefix(full_path: &[String]) -> &[String] {
    let start = full_path
        .iter()
        .take_while(|segment| is_relative_keyword(segment))
        .count();
    &full_path[start..]
}

fn is_nonbinding_import(name: &str) -> bool {
    name == "_" || is_relative_keyword(name)
}

fn is_relative_keyword(segment: &str) -> bool {
    matches!(segment, "crate" | "self" | "super")
}

fn canonical_parent_surface_candidate(
    path: &Path,
    current_module_path: &[String],
    import_path: &[String],
    binding_name: &str,
    settings: &NamespaceSettings,
) -> Option<String> {
    if import_path.len() < 2 || current_module_path.is_empty() {
        return None;
    }

    let imported_parent = import_path.iter().rev().nth(1)?;
    let imported_parent_normalized = imported_parent.to_ascii_lowercase();
    if !settings
        .organizational_modules
        .contains(&imported_parent_normalized)
        && !settings.generic_nouns.contains(binding_name)
    {
        return None;
    }

    let parent_surface_path = &current_module_path[..current_module_path.len() - 1];
    if import_path.len() == parent_surface_path.len() + 1
        && import_path[..import_path.len() - 1] == *parent_surface_path
    {
        return None;
    }

    let public_bindings = public_bindings_for_module(path, parent_surface_path);
    if !public_bindings.contains(binding_name) {
        return None;
    }

    Some(render_canonical_parent_surface(
        path,
        parent_surface_path,
        binding_name,
    ))
}

fn canonical_parent_surface_message(
    binding_name: &str,
    source_name: &str,
    parent_module: &str,
    canonical_parent_surface: &str,
) -> (&'static str, String) {
    (
        "namespace_parent_surface",
        format!(
            "import bypasses the canonical parent surface for `{binding_name}` via `{parent_module}::{source_name}`; prefer `{canonical_parent_surface}`"
        ),
    )
}

fn public_bindings_for_module(path: &Path, module_path: &[String]) -> BTreeSet<String> {
    let Some(src_root) = source_root(path) else {
        return BTreeSet::new();
    };

    for candidate in parent_module_files(&src_root, module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        return collect_public_bindings(&parsed.items);
    }

    BTreeSet::new()
}

fn collect_public_bindings(items: &[Item]) -> BTreeSet<String> {
    let mut bindings = BTreeSet::new();

    for item in items {
        match item {
            Item::Use(item_use) if !matches!(item_use.vis, Visibility::Inherited) => {
                let mut leaves = Vec::new();
                flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
                for leaf in leaves {
                    if matches!(leaf.kind, UseLeafKind::Glob) {
                        continue;
                    }
                    if let Some(binding_name) = leaf.binding_name
                        && !is_nonbinding_import(&binding_name)
                    {
                        bindings.insert(binding_name);
                    }
                }
            }
            _ => {
                if let Some((binding_name, is_public)) = public_item_binding(item)
                    && is_public
                {
                    bindings.insert(binding_name);
                }
            }
        }
    }

    bindings
}

fn public_item_binding(item: &Item) -> Option<(String, bool)> {
    match item {
        Item::Struct(ItemStruct { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Enum(ItemEnum { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Trait(ItemTrait { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::TraitAlias(ItemTraitAlias { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Type(ItemType { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Union(ItemUnion { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Fn(ItemFn { sig, vis, .. }) => {
            Some((sig.ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Const(ItemConst { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        Item::Static(ItemStatic { ident, vis, .. }) => {
            Some((ident.to_string(), !matches!(vis, Visibility::Inherited)))
        }
        _ => None,
    }
}

fn render_canonical_parent_surface(
    path: &Path,
    module_path: &[String],
    binding_name: &str,
) -> String {
    if module_path.is_empty() {
        if let Some(package_name) = package_name_for_file(path) {
            return format!("{package_name}::{binding_name}");
        }
        return format!("crate::{binding_name}");
    }

    format!("{}::{binding_name}", module_path.join("::"))
}

fn package_name_for_file(path: &Path) -> Option<String> {
    let package_root = find_package_root(path)?;
    let manifest = fs::read_to_string(package_root.join("Cargo.toml")).ok()?;
    let manifest = toml::from_str::<toml::Value>(&manifest).ok()?;
    let package_name = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("name"))
        .and_then(toml::Value::as_str)?;
    Some(package_name.replace('-', "_"))
}

fn find_package_root(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors().skip(1) {
        let manifest_path = ancestor.join("Cargo.toml");
        if manifest_path.is_file()
            && let Ok(manifest_src) = fs::read_to_string(&manifest_path)
            && let Ok(manifest) = toml::from_str::<toml::Value>(&manifest_src)
            && manifest.get("package").is_some_and(toml::Value::is_table)
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn inferred_file_module_path(path: &Path) -> Vec<String> {
    let components = path
        .iter()
        .map(|component| component.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let rel = if let Some(src_idx) = components.iter().rposition(|component| component == "src") {
        &components[src_idx + 1..]
    } else {
        &components[..]
    };

    if rel.is_empty() || rel.first().is_some_and(|component| component == "bin") {
        return Vec::new();
    }

    let mut module_path = Vec::new();
    for (idx, component) in rel.iter().enumerate() {
        let is_last = idx + 1 == rel.len();
        if is_last {
            match component.as_str() {
                "lib.rs" | "main.rs" | "mod.rs" => {}
                other => {
                    if let Some(stem) = other.strip_suffix(".rs") {
                        module_path.push(stem.to_string());
                    }
                }
            }
            continue;
        }

        module_path.push(component.to_string());
    }

    module_path
}

fn source_root(path: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    for component in path.components() {
        root.push(component.as_os_str());
        if component.as_os_str() == "src" {
            return Some(root);
        }
    }
    None
}

fn parent_module_files(src_root: &Path, prefix: &[String]) -> Vec<PathBuf> {
    if prefix.is_empty() {
        return vec![src_root.join("lib.rs"), src_root.join("main.rs")];
    }

    let joined = prefix.join("/");
    vec![
        src_root.join(format!("{joined}.rs")),
        src_root.join(joined).join("mod.rs"),
    ]
}

#[derive(Clone, Copy)]
enum NameStyle {
    Pascal,
    Snake,
    ScreamingSnake,
}

fn detect_name_style(name: &str) -> NameStyle {
    if name.contains('_') {
        if name
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic())
            .all(|ch| ch.is_ascii_uppercase())
        {
            NameStyle::ScreamingSnake
        } else {
            NameStyle::Snake
        }
    } else {
        NameStyle::Pascal
    }
}

fn render_segments(segments: &[String], style: NameStyle) -> String {
    match style {
        NameStyle::Pascal => segments
            .iter()
            .map(|segment| {
                let lower = segment.to_ascii_lowercase();
                let mut chars = lower.chars();
                let Some(first) = chars.next() else {
                    return String::new();
                };
                let mut rendered = String::new();
                rendered.push(first.to_ascii_uppercase());
                rendered.extend(chars);
                rendered
            })
            .collect::<Vec<_>>()
            .join(""),
        NameStyle::Snake => segments
            .iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("_"),
        NameStyle::ScreamingSnake => segments
            .iter()
            .map(|segment| segment.to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
    }
}

fn module_path_contains_namespace(module_path: &[String], namespace: &str) -> bool {
    module_path
        .iter()
        .any(|segment| segment.eq_ignore_ascii_case(namespace))
}

fn direct_child_module_is_private(path: &Path, analysis_path: &[String]) -> bool {
    if analysis_path.len() != 2 {
        return false;
    }

    let Some(child_name) = analysis_path.first() else {
        return false;
    };

    child_module_visibility_in_file(path, child_name) == Some(false)
}

fn child_module_visibility_in_file(path: &Path, child_name: &str) -> Option<bool> {
    let src = fs::read_to_string(path).ok()?;
    let parsed = syn::parse_file(&src).ok()?;

    parsed.items.into_iter().find_map(|item| match item {
        Item::Mod(item_mod) if item_mod.ident == child_name => {
            Some(!matches!(item_mod.vis, Visibility::Inherited))
        }
        _ => None,
    })
}

fn split_segments(name: &str) -> Vec<String> {
    if name.contains('_') {
        return name
            .split('_')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
    }

    let chars: Vec<(usize, char)> = name.char_indices().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut starts = vec![0usize];
    for i in 1..chars.len() {
        let prev = chars[i - 1].1;
        let curr = chars[i].1;
        let next = chars.get(i + 1).map(|(_, c)| *c);

        let lower_to_upper = prev.is_ascii_lowercase() && curr.is_ascii_uppercase();
        let acronym_to_word = prev.is_ascii_uppercase()
            && curr.is_ascii_uppercase()
            && next.map(|c| c.is_ascii_lowercase()).unwrap_or(false);

        if lower_to_upper || acronym_to_word {
            starts.push(chars[i].0);
        }
    }

    let mut out = Vec::with_capacity(starts.len());
    for (idx, start) in starts.iter().enumerate() {
        let end = if let Some(next) = starts.get(idx + 1) {
            *next
        } else {
            name.len()
        };
        let seg = &name[*start..end];
        if !seg.is_empty() {
            out.push(seg.to_string());
        }
    }
    out
}
