use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use syn::{
    File, Item, ItemConst, ItemEnum, ItemFn, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemTraitAlias, ItemType, ItemUnion, ItemUse, UseTree, spanned::Spanned,
};

use super::{
    Diagnostic, DiagnosticLevel, NamespaceSettings, is_public, normalize_segment, split_segments,
    unraw_ident,
};

pub(super) struct ApiShapeAnalysis {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone)]
struct PublicUseLeaf {
    binding_name: String,
    full_path: Vec<String>,
}

#[derive(Clone)]
struct PublicLeafBinding {
    line: usize,
    binding_name: String,
}

#[derive(Clone)]
struct TailSemanticFamilyMember {
    line: usize,
    original_member: String,
    suggested_leaf: String,
    child_module_name: Option<String>,
}

#[derive(Clone)]
struct ChildModuleSurfaceExport {
    parent_binding: String,
    child_leaf: String,
}

struct ScopeSurfaceContext<'a> {
    public_bindings: &'a BTreeSet<String>,
    suppressed_child_module_exports: &'a BTreeSet<String>,
}

#[derive(Clone, Copy)]
enum NameStyle {
    Pascal,
    Snake,
    ScreamingSnake,
}

pub(super) fn analyze_api_shape_rules(
    path: &Path,
    parsed: &File,
    settings: &NamespaceSettings,
) -> ApiShapeAnalysis {
    let inferred_module_path = inferred_file_module_path(path);
    let inferred_is_public =
        inferred_module_path.is_empty() || inferred_module_is_public(path, &inferred_module_path);
    let mut diagnostics = Vec::new();

    if inferred_is_public
        && let Some(module_name) = inferred_module_path.last()
        && settings
            .organizational_modules
            .contains(&normalize_segment(module_name))
        && let Some(flatten_leaf) = organizational_flatten_candidate(Some(&parsed.items), settings)
    {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Error,
            file: Some(path.to_path_buf()),
            line: Some(1),
            code: Some("api_organizational_submodule_flatten".to_string()),
            policy: true,
            fix: None,
            message: format!(
                "`{}` leaks organizational `{}` into the public API; prefer `{}` and keep `{}` private",
                render_public_path(
                    &inferred_module_path,
                    &flatten_leaf
                ),
                inferred_module_path.last().expect("non-empty inferred module path"),
                render_preferred_public_path(
                    &inferred_module_path[..inferred_module_path.len() - 1],
                    &flatten_leaf,
                    settings,
                ),
                inferred_module_path.last().expect("non-empty inferred module path"),
            ),
        });
    }

    analyze_scope(
        path,
        &parsed.items,
        &inferred_module_path,
        inferred_is_public,
        settings,
        &mut diagnostics,
    );

    ApiShapeAnalysis { diagnostics }
}

fn analyze_scope(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    path_is_public: bool,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let public_bindings = collect_scope_public_bindings(items);
    let suppressed_child_module_exports = analyze_candidate_semantic_modules(
        path,
        items,
        module_path,
        path_is_public,
        &public_bindings,
        settings,
        diagnostics,
    );
    let scope_context = ScopeSurfaceContext {
        public_bindings: &public_bindings,
        suppressed_child_module_exports: &suppressed_child_module_exports,
    };

    for item in items {
        match item {
            Item::Mod(item_mod) => analyze_module_item(
                path,
                item_mod,
                module_path,
                path_is_public,
                &scope_context,
                settings,
                diagnostics,
            ),
            Item::Use(item_use) => {
                if path_is_public {
                    analyze_public_use_item(
                        path,
                        item_use,
                        items,
                        module_path,
                        &scope_context,
                        settings,
                        diagnostics,
                    );
                }
            }
            _ => analyze_public_item(
                path,
                item,
                items,
                module_path,
                path_is_public,
                settings,
                diagnostics,
            ),
        }
    }
}

fn analyze_module_item(
    path: &Path,
    item_mod: &ItemMod,
    module_path: &[String],
    path_is_public: bool,
    scope_context: &ScopeSurfaceContext<'_>,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let module_name = item_mod.ident.to_string();
    let normalized = normalize_segment(&module_name);
    let line = item_mod.span().start().line;
    let module_is_public = path_is_public && is_public(&item_mod.vis);

    if module_is_public {
        if settings.catch_all_modules.contains(&normalized) {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                file: Some(path.to_path_buf()),
                line: Some(line),
                code: Some("api_catch_all_module".to_string()),
                policy: true,
                fix: None,
                message: format!(
                    "`{module_name}` is a catch-all public module; prefer a stable domain or facet"
                ),
            });
        }

        if settings.organizational_modules.contains(&normalized)
            && let Some(flatten_leaf) = organizational_flatten_candidate(
                item_mod.content.as_ref().map(|(_, nested)| nested),
                settings,
            )
        {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Error,
                file: Some(path.to_path_buf()),
                line: Some(line),
                code: Some("api_organizational_submodule_flatten".to_string()),
                policy: true,
                fix: None,
                message: format!(
                    "`{}` leaks organizational `{module_name}` into the public API; prefer `{}` and keep `{module_name}` private",
                    render_public_path_with_module(module_path, &module_name, &flatten_leaf),
                    render_preferred_public_path(module_path, &flatten_leaf, settings)
                ),
            });
        }

        if module_path
            .last()
            .is_some_and(|parent| normalize_segment(parent) == normalized)
        {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                file: Some(path.to_path_buf()),
                line: Some(line),
                code: Some("api_repeated_module_segment".to_string()),
                policy: true,
                fix: None,
                message: format!(
                    "nested module path repeats `{module_name}`; flatten or rename the redundant segment"
                ),
            });
        }
    }

    if is_surface_export_candidate(module_path, path_is_public, settings)
        && is_public(&item_mod.vis)
        && !scope_context
            .suppressed_child_module_exports
            .contains(&normalized)
        && let Some(surface_export) = child_module_surface_export_candidate(
            path,
            module_path,
            &module_name,
            item_mod
                .content
                .as_ref()
                .map(|(_, nested)| nested.as_slice()),
            settings,
        )
        && !scope_context
            .public_bindings
            .contains(&surface_export.parent_binding)
    {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_missing_parent_surface_export".to_string()),
            policy: true,
            fix: None,
            message: format!(
                "parent surface is missing `{}`; re-export it so callers do not have to use `{}`",
                render_public_path(module_path, &surface_export.parent_binding),
                render_public_path_with_module(
                    module_path,
                    &module_name,
                    &surface_export.child_leaf,
                ),
            ),
        });
    }

    if let Some((_, nested)) = &item_mod.content {
        let mut next_path = module_path.to_vec();
        next_path.push(module_name);
        analyze_scope(
            path,
            nested,
            &next_path,
            module_is_public,
            settings,
            diagnostics,
        );
    }
}

fn organizational_flatten_candidate(
    nested: Option<&Vec<Item>>,
    settings: &NamespaceSettings,
) -> Option<String> {
    let nested = nested?;
    let mut public_leaf = None;

    for item in nested {
        if public_item_leaf(item).is_some_and(|(_, _, is_item_public)| is_item_public) {
            let (_, leaf_name, _) = public_item_leaf(item)?;
            if public_leaf.replace(leaf_name).is_some() {
                return None;
            }
            continue;
        }

        match item {
            Item::Mod(item_mod) if is_public(&item_mod.vis) => return None,
            Item::Use(item_use) if is_public(&item_use.vis) => return None,
            _ => {}
        }
    }

    let leaf_name = public_leaf?;
    if split_segments(&leaf_name).len() == 1 && settings.generic_nouns.contains(&leaf_name) {
        Some(leaf_name)
    } else {
        None
    }
}

fn analyze_public_use_item(
    path: &Path,
    item_use: &ItemUse,
    scope_items: &[Item],
    module_path: &[String],
    scope_context: &ScopeSurfaceContext<'_>,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_use.vis) {
        return;
    }

    let mut leaves = Vec::new();
    flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
    let line = item_use.span().start().line;

    for leaf in leaves {
        if is_surface_export_candidate(module_path, true, settings)
            && !scope_context
                .suppressed_child_module_exports
                .contains(&normalize_segment(&leaf.binding_name))
            && let Some(surface_export) = public_use_module_binding(
                path,
                module_path,
                &leaf,
                scope_context.public_bindings,
                settings,
            )
        {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                file: Some(path.to_path_buf()),
                line: Some(line),
                code: Some("api_missing_parent_surface_export".to_string()),
                policy: true,
                fix: None,
                message: format!(
                    "parent surface is missing `{}`; re-export it so callers do not have to use `{}`",
                    render_public_path(module_path, &surface_export.parent_binding),
                    render_public_path_with_module(
                        module_path,
                        &leaf.binding_name,
                        &surface_export.child_leaf,
                    ),
                ),
            });
        }

        analyze_public_leaf(
            path,
            line,
            scope_items,
            module_path,
            &leaf.binding_name,
            settings,
            diagnostics,
        );
    }
}

fn is_surface_export_candidate(
    module_path: &[String],
    path_is_public: bool,
    settings: &NamespaceSettings,
) -> bool {
    (!module_path.is_empty() && path_is_public)
        || module_path.last().is_some_and(|segment| {
            settings
                .namespace_preserving_modules
                .contains(&normalize_segment(segment))
        })
}

fn collect_scope_public_bindings(items: &[Item]) -> BTreeSet<String> {
    let mut bindings = BTreeSet::new();

    for item in items {
        match item {
            Item::Use(item_use) if is_public(&item_use.vis) => {
                let mut leaves = Vec::new();
                flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
                for leaf in leaves {
                    bindings.insert(leaf.binding_name);
                }
            }
            _ => {
                if let Some((_, leaf_name, is_item_public)) = public_item_leaf(item)
                    && is_item_public
                {
                    bindings.insert(leaf_name);
                }
            }
        }
    }

    bindings
}

fn collect_scope_public_leaf_bindings(items: &[Item]) -> Vec<PublicLeafBinding> {
    let mut bindings = Vec::new();

    for item in items {
        match item {
            Item::Use(item_use) if is_public(&item_use.vis) => {
                let mut leaves = Vec::new();
                flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
                let line = item_use.span().start().line;
                for leaf in leaves {
                    bindings.push(PublicLeafBinding {
                        line,
                        binding_name: leaf.binding_name,
                    });
                }
            }
            _ => {
                if let Some((line, leaf_name, is_item_public)) = public_item_leaf(item)
                    && is_item_public
                {
                    bindings.push(PublicLeafBinding {
                        line,
                        binding_name: leaf_name,
                    });
                }
            }
        }
    }

    bindings
}

fn analyze_candidate_semantic_modules(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    path_is_public: bool,
    public_bindings: &BTreeSet<String>,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut suppressed_child_module_exports = BTreeSet::new();
    if !path_is_public {
        return suppressed_child_module_exports;
    }

    let child_modules = semantic_child_module_bindings(path, items, module_path, settings);
    let public_leaves = collect_scope_public_leaf_bindings(items);
    let mut families = BTreeMap::<String, Vec<(usize, String, String)>>::new();

    for binding in &public_leaves {
        if !matches!(detect_name_style(&binding.binding_name), NameStyle::Pascal) {
            continue;
        }

        let segments = split_segments(&binding.binding_name);
        if segments.len() < 2 {
            continue;
        }

        let head = segments[0].clone();
        let module_candidate = head.to_ascii_lowercase();
        if settings.weak_modules.contains(&module_candidate)
            || settings.catch_all_modules.contains(&module_candidate)
            || settings.organizational_modules.contains(&module_candidate)
            || child_modules
                .keys()
                .any(|module_name| normalize_segment(module_name) == normalize_segment(&head))
        {
            continue;
        }

        let style = detect_name_style(&binding.binding_name);
        let shorter_leaf = render_segments(&segments[1..], style);
        families.entry(head).or_default().push((
            binding.line,
            binding.binding_name.clone(),
            shorter_leaf,
        ));
    }

    for (head, members) in families {
        if members.len() < 2 {
            continue;
        }

        let line = members
            .iter()
            .map(|(line, _, _)| *line)
            .min()
            .expect("family has at least one member");
        let original_members = members
            .iter()
            .map(|(_, binding_name, _)| format!("`{binding_name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let suggested_members = members
            .iter()
            .map(|(_, _, shorter_leaf)| shorter_leaf.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        let module_candidate = head.to_ascii_lowercase();

        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_candidate_semantic_module".to_string()),
            policy: false,
            fix: None,
            message: format!(
                "public siblings {original_members} share the `{head}` head; consider a semantic `{module_candidate}::{{{suggested_members}}}` surface"
            ),
        });
    }

    for tail in public_bindings {
        if !settings.generic_nouns.contains(tail) {
            continue;
        }

        let module_candidate = tail.to_ascii_lowercase();
        if settings.weak_modules.contains(&module_candidate)
            || settings.catch_all_modules.contains(&module_candidate)
            || settings.organizational_modules.contains(&module_candidate)
            || child_modules
                .keys()
                .any(|module_name| normalize_segment(module_name) == normalize_segment(tail))
        {
            continue;
        }

        let mut members = Vec::<TailSemanticFamilyMember>::new();

        for binding in &public_leaves {
            if !matches!(detect_name_style(&binding.binding_name), NameStyle::Pascal) {
                continue;
            }

            let segments = split_segments(&binding.binding_name);
            if segments.len() < 2 {
                continue;
            }

            let last_segment = segments.last().expect("len checked");
            if normalize_segment(last_segment) != normalize_segment(tail) {
                continue;
            }

            let shorter_leaf = render_segments(
                &segments[..segments.len() - 1],
                detect_name_style(&binding.binding_name),
            );
            members.push(TailSemanticFamilyMember {
                line: binding.line,
                original_member: binding.binding_name.clone(),
                suggested_leaf: shorter_leaf,
                child_module_name: None,
            });
        }

        for item in items {
            let Item::Mod(item_mod) = item else {
                continue;
            };
            if !is_public(&item_mod.vis) {
                continue;
            }

            let module_name = item_mod.ident.to_string();
            let Some(bindings) = child_modules.get(&module_name) else {
                continue;
            };
            if bindings.len() != 1 {
                continue;
            }

            let child_leaf = bindings.iter().next().expect("len checked");
            if normalize_segment(child_leaf) != normalize_segment(tail) {
                continue;
            }

            members.push(TailSemanticFamilyMember {
                line: item_mod.span().start().line,
                original_member: format!("{module_name}::{child_leaf}"),
                suggested_leaf: render_segments(&split_segments(&module_name), NameStyle::Pascal),
                child_module_name: Some(module_name),
            });
        }

        if members.len() < 2 {
            continue;
        }

        let line = members
            .iter()
            .map(|member| member.line)
            .min()
            .expect("family has at least one member");
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.original_member))
            .collect::<Vec<_>>()
            .join(", ");
        let suggested_members = members
            .iter()
            .map(|member| member.suggested_leaf.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");

        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_candidate_semantic_module".to_string()),
            policy: false,
            fix: None,
            message: format!(
                "public siblings {original_members} share the generic `{tail}` tail; consider a semantic `{module_candidate}::{{{suggested_members}}}` surface"
            ),
        });

        for member in members {
            if let Some(module_name) = member.child_module_name {
                suppressed_child_module_exports.insert(normalize_segment(&module_name));
            }
        }
    }

    suppressed_child_module_exports
}

fn public_use_module_binding(
    path: &Path,
    module_path: &[String],
    leaf: &PublicUseLeaf,
    public_bindings: &BTreeSet<String>,
    settings: &NamespaceSettings,
) -> Option<ChildModuleSurfaceExport> {
    let resolved_module_path = resolve_local_module_path(module_path, &leaf.full_path)?;
    let module_name = resolved_module_path.last()?;
    if normalize_segment(module_name) != normalize_segment(&leaf.binding_name) {
        return None;
    }

    let surface_export = child_module_surface_export_candidate(
        path,
        &resolved_module_path[..resolved_module_path.len() - 1],
        module_name,
        None,
        settings,
    )?;
    (!public_bindings.contains(&surface_export.parent_binding)).then_some(surface_export)
}

fn child_module_surface_export_candidate(
    path: &Path,
    module_path: &[String],
    module_name: &str,
    inline_items: Option<&[Item]>,
    settings: &NamespaceSettings,
) -> Option<ChildModuleSurfaceExport> {
    if settings
        .organizational_modules
        .contains(&normalize_segment(module_name))
    {
        return None;
    }

    if let Some(items) = inline_items {
        return matching_child_module_surface_export(items, module_name, settings);
    }

    let src_root = source_root(path)?;
    let mut full_module_path = module_path.to_vec();
    full_module_path.push(module_name.to_string());
    matching_child_module_surface_export_from_files(
        path,
        &src_root,
        &full_module_path,
        module_name,
        settings,
    )
}

fn matching_child_module_surface_export(
    items: &[Item],
    module_name: &str,
    settings: &NamespaceSettings,
) -> Option<ChildModuleSurfaceExport> {
    if let Some(matching_leaf) = matching_child_module_leaf(items, module_name) {
        return Some(ChildModuleSurfaceExport {
            parent_binding: matching_leaf.clone(),
            child_leaf: matching_leaf,
        });
    }

    let child_leaf = sole_public_generic_binding(items, settings)?;
    let parent_binding = render_segments(&split_segments(module_name), NameStyle::Pascal);
    (normalize_segment(&parent_binding) != normalize_segment(&child_leaf)).then_some(
        ChildModuleSurfaceExport {
            parent_binding,
            child_leaf,
        },
    )
}

fn matching_child_module_surface_export_from_files(
    current_file: &Path,
    src_root: &Path,
    module_path: &[String],
    module_name: &str,
    settings: &NamespaceSettings,
) -> Option<ChildModuleSurfaceExport> {
    for candidate in parent_module_files(src_root, module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        if let Some(matching) =
            matching_child_module_surface_export(&parsed.items, module_name, settings)
        {
            return Some(matching);
        }
    }

    let parent_module_path = &module_path[..module_path.len().checked_sub(1)?];
    let parent_items = load_module_items(current_file, parent_module_path)?;
    for item in parent_items {
        let Item::Use(item_use) = item else {
            continue;
        };
        if !is_public(&item_use.vis) {
            continue;
        }

        let mut leaves = Vec::new();
        flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
        for leaf in leaves {
            if leaf.binding_name != module_name {
                continue;
            }
            let resolved = resolve_local_module_path(parent_module_path, &leaf.full_path)?;
            if resolved == module_path {
                continue;
            }
            if let Some(matching) = matching_child_module_surface_export_from_files(
                current_file,
                src_root,
                &resolved,
                resolved.last()?,
                settings,
            ) {
                return Some(matching);
            }
        }
    }

    None
}

fn sole_public_generic_binding(items: &[Item], settings: &NamespaceSettings) -> Option<String> {
    let mut public_leaf = None;

    for item in items {
        if let Some((_, leaf_name, is_item_public)) = public_item_leaf(item)
            && is_item_public
        {
            if public_leaf.replace(leaf_name).is_some() {
                return None;
            }
            continue;
        }

        match item {
            Item::Use(item_use) if is_public(&item_use.vis) => {
                let mut leaves = Vec::new();
                flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
                for leaf in leaves {
                    if public_leaf.replace(leaf.binding_name).is_some() {
                        return None;
                    }
                }
            }
            Item::Mod(item_mod) if is_public(&item_mod.vis) => return None,
            _ => {}
        }
    }

    let leaf_name = public_leaf?;
    (split_segments(&leaf_name).len() == 1 && settings.generic_nouns.contains(&leaf_name))
        .then_some(leaf_name)
}

fn load_module_items(current_file: &Path, module_path: &[String]) -> Option<Vec<Item>> {
    let src_root = source_root(current_file)?;
    for candidate in parent_module_files(&src_root, module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        return Some(parsed.items);
    }

    None
}

fn matching_child_module_leaf(items: &[Item], module_name: &str) -> Option<String> {
    let mut matching = BTreeSet::new();
    let normalized_module = normalize_segment(module_name);

    for item in items {
        if let Some((_, leaf_name, is_item_public)) = public_item_leaf(item)
            && is_item_public
            && normalize_segment(&leaf_name) == normalized_module
        {
            matching.insert(leaf_name);
        }

        if let Item::Use(item_use) = item
            && is_public(&item_use.vis)
        {
            let mut leaves = Vec::new();
            flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
            for leaf in leaves {
                if normalize_segment(&leaf.binding_name) == normalized_module {
                    matching.insert(leaf.binding_name);
                }
            }
        }
    }

    (matching.len() == 1).then(|| matching.into_iter().next().expect("one binding"))
}

fn resolve_local_module_path(module_path: &[String], use_path: &[String]) -> Option<Vec<String>> {
    if use_path.is_empty() {
        return None;
    }

    let mut base = module_path.to_vec();
    let mut iter = use_path.iter();
    let mut saw_root = false;

    while let Some(segment) = iter.next() {
        match segment.as_str() {
            "crate" => {
                base.clear();
                saw_root = true;
            }
            "self" => {}
            "super" => {
                base.pop()?;
            }
            other => {
                let mut resolved = if saw_root { Vec::new() } else { base };
                resolved.push(other.to_string());
                resolved.extend(iter.cloned());
                return Some(resolved);
            }
        }
    }

    None
}

fn analyze_public_item(
    path: &Path,
    item: &Item,
    scope_items: &[Item],
    module_path: &[String],
    path_is_public: bool,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((line, leaf_name, is_item_public)) = public_item_leaf(item) else {
        return;
    };
    if !(path_is_public && is_item_public) {
        return;
    }

    analyze_public_leaf(
        path,
        line,
        scope_items,
        module_path,
        &leaf_name,
        settings,
        diagnostics,
    );
}

fn analyze_public_leaf(
    path: &Path,
    line: usize,
    scope_items: &[Item],
    module_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(preferred_path) =
        semantic_module_surface_candidate(path, scope_items, module_path, leaf_name, settings)
    {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_redundant_leaf_context".to_string()),
            policy: true,
            fix: None,
            message: format!(
                "public API already exposes `{preferred_path}`; prefer it over `{}`",
                render_public_path(module_path, leaf_name),
            ),
        });
        return;
    }

    let Some(parent_module) = module_path.last() else {
        return;
    };
    let parent_normalized = normalize_segment(parent_module);

    if settings.weak_modules.contains(&parent_normalized)
        && settings.generic_nouns.contains(leaf_name)
    {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_weak_module_generic_leaf".to_string()),
            policy: true,
            fix: None,
            message: format!(
                "`{}` is too generic for weak module `{parent_module}`; keep the domain in the leaf or choose a stronger module",
                render_public_path(module_path, leaf_name),
            ),
        });
        return;
    }

    if let Some(shorter_leaf) = redundant_category_suffix_leaf(parent_module, leaf_name, settings) {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_redundant_category_suffix".to_string()),
            policy: true,
            fix: None,
            message: format!(
                "`{}` repeats the `{parent_module}` category; prefer `{}`",
                render_public_path(module_path, leaf_name),
                render_preferred_public_path(module_path, &shorter_leaf, settings)
            ),
        });
        return;
    }

    if settings.weak_modules.contains(&parent_normalized)
        || settings.catch_all_modules.contains(&parent_normalized)
    {
        return;
    }

    if let Some(shorter_leaf) = redundant_leaf_context_candidate(parent_module, leaf_name) {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: Some(path.to_path_buf()),
            line: Some(line),
            code: Some("api_redundant_leaf_context".to_string()),
            policy: true,
            fix: None,
            message: format!(
                "`{}` repeats the `{parent_module}` context; prefer `{}`",
                render_public_path(module_path, leaf_name),
                render_preferred_public_path(module_path, &shorter_leaf, settings)
            ),
        });
    }
}

fn render_preferred_public_path(
    module_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> String {
    let normalized_modules = normalize_generic_surface_modules(module_path, leaf_name, settings);
    render_public_path(&normalized_modules, leaf_name)
}

fn normalize_generic_surface_modules(
    module_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> Vec<String> {
    let mut modules = module_path.to_vec();

    while let Some(last_module) = modules.last() {
        let last_normalized = normalize_segment(last_module);
        let should_drop = (settings.organizational_modules.contains(&last_normalized)
            && settings.generic_nouns.contains(leaf_name))
            || (settings.generic_nouns.contains(leaf_name)
                && normalize_segment(leaf_name) == last_normalized);
        if !should_drop {
            break;
        }
        modules.pop();
    }

    modules
}

fn semantic_module_surface_candidate(
    path: &Path,
    scope_items: &[Item],
    module_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> Option<String> {
    let child_module_bindings =
        semantic_child_module_bindings(path, scope_items, module_path, settings);
    if child_module_bindings.is_empty() {
        return None;
    }

    let leaf_segments = split_segments(leaf_name);
    if leaf_segments.len() < 2 {
        return None;
    }

    let leaf_normalized = leaf_segments
        .iter()
        .map(|segment| normalize_segment(segment))
        .collect::<Vec<_>>();
    let style = detect_name_style(leaf_name);

    for (module_name, bindings) in child_module_bindings {
        let module_segments = split_segments(&module_name)
            .into_iter()
            .map(|segment| normalize_segment(&segment))
            .collect::<Vec<_>>();
        if module_segments.is_empty() {
            continue;
        }

        if leaf_normalized.starts_with(&module_segments) {
            let shorter_segments = &leaf_segments[module_segments.len()..];
            if shorter_segments.is_empty() {
                continue;
            }

            let shorter_leaf = render_segments(shorter_segments, style);
            if !bindings
                .iter()
                .any(|binding| normalize_segment(binding) == normalize_segment(&shorter_leaf))
            {
                continue;
            }

            return Some(render_public_path_with_module(
                module_path,
                &module_name,
                &shorter_leaf,
            ));
        }

        if !leaf_normalized.ends_with(&module_segments) {
            continue;
        }

        let shorter_segments = &leaf_segments[..leaf_segments.len() - module_segments.len()];
        if shorter_segments.is_empty() {
            continue;
        }

        let shorter_leaf = render_segments(shorter_segments, style);
        if !bindings
            .iter()
            .any(|binding| normalize_segment(binding) == normalize_segment(&shorter_leaf))
        {
            continue;
        }

        return Some(render_public_path_with_module(
            module_path,
            &module_name,
            &shorter_leaf,
        ));
    }

    None
}

fn semantic_child_module_bindings(
    path: &Path,
    scope_items: &[Item],
    module_path: &[String],
    settings: &NamespaceSettings,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();

    for item in scope_items {
        let Item::Mod(item_mod) = item else {
            continue;
        };
        if !is_public(&item_mod.vis) {
            continue;
        }

        let module_name = item_mod.ident.to_string();
        let normalized = normalize_segment(&module_name);
        if settings.weak_modules.contains(&normalized)
            || settings.catch_all_modules.contains(&normalized)
            || settings.organizational_modules.contains(&normalized)
        {
            continue;
        }

        let child_bindings = public_bindings_for_child_module(
            path,
            module_path,
            &module_name,
            item_mod
                .content
                .as_ref()
                .map(|(_, nested)| nested.as_slice()),
        );
        if child_bindings.is_empty() {
            continue;
        }

        out.insert(module_name, child_bindings);
    }

    out
}

fn public_bindings_for_child_module(
    current_file: &Path,
    module_path: &[String],
    module_name: &str,
    inline_items: Option<&[Item]>,
) -> BTreeSet<String> {
    if let Some(items) = inline_items {
        return collect_scope_public_bindings(items);
    }

    let Some(src_root) = source_root(current_file) else {
        return BTreeSet::new();
    };

    let mut full_module_path = module_path.to_vec();
    full_module_path.push(module_name.to_string());

    for candidate in parent_module_files(&src_root, &full_module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        return collect_scope_public_bindings(&parsed.items);
    }

    BTreeSet::new()
}

fn public_item_leaf(item: &Item) -> Option<(usize, String, bool)> {
    match item {
        Item::Struct(ItemStruct { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::Enum(ItemEnum { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::Trait(ItemTrait { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::TraitAlias(ItemTraitAlias { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::Type(ItemType { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::Union(ItemUnion { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::Fn(ItemFn { sig, vis, .. }) => Some((
            item.span().start().line,
            unraw_ident(&sig.ident),
            is_public(vis),
        )),
        Item::Const(ItemConst { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        Item::Static(ItemStatic { ident, vis, .. }) => {
            Some((item.span().start().line, unraw_ident(ident), is_public(vis)))
        }
        _ => None,
    }
}

fn redundant_category_suffix_leaf(
    parent_module: &str,
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> Option<String> {
    let leaf_segments = split_segments(leaf_name);
    if leaf_segments.len() < 2 {
        return None;
    }

    let parent_normalized = normalize_segment(parent_module);
    let style = detect_name_style(leaf_name);
    let last_segment = leaf_segments.last().map(|segment| segment.to_string())?;

    for noun in &settings.generic_nouns {
        if normalize_segment(noun) != parent_normalized {
            continue;
        }
        if normalize_segment(&last_segment) != normalize_segment(noun) {
            continue;
        }

        let shorter_segments = &leaf_segments[..leaf_segments.len() - 1];
        if shorter_segments.is_empty() {
            return None;
        }

        return Some(render_segments(shorter_segments, style));
    }

    None
}

fn redundant_leaf_context_candidate(parent_module: &str, leaf_name: &str) -> Option<String> {
    let module_segments = split_segments(parent_module)
        .into_iter()
        .map(|segment| normalize_segment(&segment))
        .collect::<Vec<_>>();
    let leaf_segments = split_segments(leaf_name);
    if module_segments.is_empty() || leaf_segments.len() <= module_segments.len() {
        return None;
    }

    let leaf_normalized = leaf_segments
        .iter()
        .map(|segment| normalize_segment(segment))
        .collect::<Vec<_>>();
    let style = detect_name_style(leaf_name);

    if leaf_normalized.starts_with(&module_segments) {
        let shorter_segments = &leaf_segments[module_segments.len()..];
        if !shorter_segments.is_empty() {
            return Some(render_segments(shorter_segments, style));
        }
    }

    if leaf_normalized.ends_with(&module_segments) {
        let shorter_segments = &leaf_segments[..leaf_segments.len() - module_segments.len()];
        if !shorter_segments.is_empty() {
            return Some(render_segments(shorter_segments, style));
        }
    }

    None
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

fn render_public_path(module_path: &[String], leaf_name: &str) -> String {
    if module_path.is_empty() {
        leaf_name.to_string()
    } else {
        format!("{}::{leaf_name}", module_path.join("::"))
    }
}

fn render_public_path_with_module(
    module_path: &[String],
    module_name: &str,
    leaf_name: &str,
) -> String {
    let mut full = module_path.to_vec();
    full.push(module_name.to_string());
    render_public_path(&full, leaf_name)
}

fn flatten_public_use_tree(prefix: Vec<String>, tree: &UseTree, leaves: &mut Vec<PublicUseLeaf>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            flatten_public_use_tree(next, &path.tree, leaves);
        }
        UseTree::Name(name) => {
            let binding_name = name.ident.to_string();
            if binding_name != "self" {
                let mut full_path = prefix;
                full_path.push(binding_name.clone());
                leaves.push(PublicUseLeaf {
                    binding_name,
                    full_path,
                });
            }
        }
        UseTree::Rename(rename) => {
            let mut full_path = prefix;
            full_path.push(rename.ident.to_string());
            leaves.push(PublicUseLeaf {
                binding_name: rename.rename.to_string(),
                full_path,
            });
        }
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_public_use_tree(prefix.clone(), item, leaves);
            }
        }
        UseTree::Glob(_) => {}
    }
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

fn inferred_module_is_public(path: &Path, module_path: &[String]) -> bool {
    let Some(src_root) = source_root(path) else {
        return false;
    };

    let mut prefix = Vec::<String>::new();
    for segment in module_path {
        let candidates = parent_module_files(&src_root, &prefix);
        let mut found_public = None;

        for candidate in candidates {
            let Some(is_public) = module_decl_visibility(&candidate, segment) else {
                continue;
            };
            found_public = Some(is_public);
            if is_public {
                break;
            }
        }

        match found_public {
            Some(true) => prefix.push(segment.clone()),
            _ => return false,
        }
    }

    true
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

fn module_decl_visibility(file: &Path, segment: &str) -> Option<bool> {
    let src = fs::read_to_string(file).ok()?;
    let parsed = syn::parse_file(&src).ok()?;

    parsed.items.into_iter().find_map(|item| match item {
        Item::Mod(item_mod) if item_mod.ident == segment => Some(is_public(&item_mod.vis)),
        _ => None,
    })
}
