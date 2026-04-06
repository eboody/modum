use std::{collections::BTreeSet, fs, path::Path};

use syn::{
    ExprPath, File, Item, ItemConst, ItemEnum, ItemFn, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemTraitAlias, ItemType, ItemUnion, ItemUse, Path as SynPath, TypeParamBound, TypePath,
    UseTree, Visibility,
    spanned::Spanned,
    visit::{self, Visit},
};

use super::{
    Diagnostic, NamespaceSettings, detect_name_style, inferred_file_module_path,
    parent_module_files, render_segments, replace_path_fix, source_root, split_segments,
};

pub(super) struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone)]
struct UseBinding {
    full_path: Vec<String>,
    source_name: String,
    binding_name: String,
}

#[derive(Clone)]
enum UseLeaf {
    Direct(UseBinding),
    Rename(UseBinding),
    Glob(Vec<String>),
}

#[derive(Clone)]
struct ScopeUseBinding {
    binding: UseBinding,
    renamed: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct QualifiedPathSurfaceCandidate {
    rendered_path: String,
    fixable: bool,
}

pub(super) fn analyze_namespace_rules(
    path: &Path,
    parsed: &File,
    settings: &NamespaceSettings,
) -> Analysis {
    let mut diagnostics = Vec::new();
    analyze_scope(path, &parsed.items, settings, &mut diagnostics);
    Analysis { diagnostics }
}

fn analyze_scope(
    path: &Path,
    items: &[Item],
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope_use_bindings = collect_scope_use_bindings(items);

    for item in items {
        match item {
            Item::Use(item_use) => analyze_use_item(path, items, item_use, settings, diagnostics),
            Item::Type(item_type) => {
                analyze_type_alias_item(path, item_type, settings, diagnostics);
                analyze_qualified_callsite_paths(
                    path,
                    item,
                    settings,
                    diagnostics,
                    &scope_use_bindings,
                );
            }
            Item::Mod(ItemMod {
                content: Some((_, nested)),
                ..
            }) => analyze_scope(path, nested, settings, diagnostics),
            _ => analyze_qualified_callsite_paths(
                path,
                item,
                settings,
                diagnostics,
                &scope_use_bindings,
            ),
        }
    }
}

fn analyze_qualified_callsite_paths(
    path: &Path,
    item: &Item,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
    scope_use_bindings: &[ScopeUseBinding],
) {
    let mut visitor = QualifiedCallsitePathVisitor {
        file: path,
        settings,
        diagnostics,
        scope_use_bindings,
    };
    visitor.visit_item(item);
}

struct QualifiedCallsitePathVisitor<'a> {
    file: &'a Path,
    settings: &'a NamespaceSettings,
    diagnostics: &'a mut Vec<Diagnostic>,
    scope_use_bindings: &'a [ScopeUseBinding],
}

impl<'ast> Visit<'ast> for QualifiedCallsitePathVisitor<'_> {
    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        analyze_qualified_generic_path(
            self.file,
            &node.path,
            self.settings,
            self.diagnostics,
            self.scope_use_bindings,
        );
        visit::visit_expr_path(self, node);
    }

    fn visit_type_path(&mut self, node: &'ast TypePath) {
        if node.qself.is_none() {
            analyze_qualified_generic_path(
                self.file,
                &node.path,
                self.settings,
                self.diagnostics,
                self.scope_use_bindings,
            );
        }
        visit::visit_type_path(self, node);
    }

    fn visit_type_param_bound(&mut self, node: &'ast TypeParamBound) {
        if let TypeParamBound::Trait(trait_bound) = node {
            analyze_qualified_generic_path(
                self.file,
                &trait_bound.path,
                self.settings,
                self.diagnostics,
                self.scope_use_bindings,
            );
        }
        visit::visit_type_param_bound(self, node);
    }
}

fn analyze_use_item(
    path: &Path,
    scope_items: &[Item],
    item_use: &ItemUse,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut leaves = Vec::new();
    flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
    let line = item_use.span().start().line;
    let is_reexport = !matches!(item_use.vis, Visibility::Inherited);
    let current_module_path = inferred_file_module_path(path);

    for leaf in leaves {
        if let UseLeaf::Glob(full_path) = &leaf {
            analyze_glob_use_item(path, line, is_reexport, full_path, settings, diagnostics);
            continue;
        }

        let (binding, renamed) = match &leaf {
            UseLeaf::Direct(binding) => (binding, false),
            UseLeaf::Rename(binding) => (binding, true),
            UseLeaf::Glob(_) => continue,
        };
        if is_nonbinding_import(&binding.source_name) || is_nonbinding_import(&binding.binding_name)
        {
            continue;
        }

        let analysis_path = trim_relative_prefix(&binding.full_path);
        let Some(parent_module) = analysis_path.iter().rev().nth(1).cloned() else {
            continue;
        };
        let parent_normalized = parent_module.to_ascii_lowercase();
        let redundant_leaf = redundant_leaf_context_candidate(
            analysis_path,
            &binding.binding_name,
            renamed,
            is_reexport,
            settings,
        );
        let skip_reexport = is_reexport
            && ((redundant_leaf.is_some() && direct_child_module_is_private(path, analysis_path))
                || canonical_parent_surface_reexport(
                    &current_module_path,
                    analysis_path,
                    &binding.binding_name,
                    settings,
                )
                || parent_surface_reexports_current_binding(
                    path,
                    &current_module_path,
                    &binding.binding_name,
                )
                || preserved_parent_surface_reexport(
                    &current_module_path,
                    analysis_path,
                    settings,
                ));

        if skip_reexport {
            continue;
        }

        let canonical_parent_surface = if is_reexport {
            None
        } else {
            canonical_parent_surface_candidate(
                path,
                &current_module_path,
                analysis_path,
                &binding.binding_name,
                settings,
            )
        };
        let visible_callsite_surface =
            visible_callsite_surface_candidate(analysis_path, &binding.binding_name, settings);
        let binding_used_as_namespace =
            binding_is_used_as_namespace_in_scope(scope_items, &binding.binding_name);

        let (code, message) =
            if let Some(canonical_parent_surface) = canonical_parent_surface.as_deref() {
                canonical_parent_surface_message(
                    &binding.binding_name,
                    &binding.source_name,
                    &parent_module,
                    canonical_parent_surface,
                )
            } else if let Some(shorter_leaf) = redundant_leaf {
                redundant_context_message(
                    is_reexport,
                    &parent_module,
                    &binding.binding_name,
                    &shorter_leaf,
                )
            } else if settings.generic_nouns.contains(&binding.binding_name)
                && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
            {
                generic_noun_message(is_reexport, &binding.source_name, visible_callsite_surface)
            } else if settings
                .namespace_preserving_modules
                .contains(&parent_normalized)
                && !module_path_contains_namespace(&current_module_path, &parent_normalized)
                && !binding_used_as_namespace
                && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
            {
                preserve_module_message(
                    is_reexport,
                    &binding.source_name,
                    &binding.binding_name,
                    visible_callsite_surface,
                )
            } else {
                continue;
            };

        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            code,
            message,
        ));
    }
}

fn analyze_type_alias_item(
    path: &Path,
    item_type: &ItemType,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let syn::Type::Path(type_path) = item_type.ty.as_ref() else {
        return;
    };
    if type_path.qself.is_some() {
        return;
    }

    let full_path = type_path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    if full_path.len() < 2 {
        return;
    }

    let binding_name = item_type.ident.to_string();
    if is_nonbinding_import(&binding_name) {
        return;
    }

    let analysis_path = trim_relative_prefix(&full_path);
    let Some(parent_module) = analysis_path.iter().rev().nth(1).cloned() else {
        return;
    };
    let parent_normalized = parent_module.to_ascii_lowercase();
    let current_module_path = inferred_file_module_path(path);
    let redundant_leaf =
        redundant_leaf_context_candidate(analysis_path, &binding_name, true, false, settings);
    let visible_callsite_surface =
        visible_callsite_surface_candidate(analysis_path, &binding_name, settings);

    let Some((code, message)) = (if let Some(shorter_leaf) = redundant_leaf {
        Some((
            "namespace_flat_type_alias_redundant_leaf_context",
            format!(
                "type alias `{binding_name}` keeps redundant `{parent_module}` context; prefer `{parent_module}::{shorter_leaf}` directly"
            ),
        ))
    } else if settings.generic_nouns.contains(&binding_name)
        && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
    {
        Some((
            "namespace_flat_type_alias",
            format!(
                "type alias `{binding_name}` hides namespace context for `{}`; prefer `{visible_callsite_surface}` directly",
                full_path.join("::"),
            ),
        ))
    } else if settings
        .namespace_preserving_modules
        .contains(&parent_normalized)
        && !module_path_contains_namespace(&current_module_path, &parent_normalized)
        && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
    {
        Some((
            "namespace_flat_type_alias_preserve_module",
            format!(
                "type alias `{binding_name}` hides configured namespace context for `{}`; prefer `{visible_callsite_surface}` directly",
                full_path.join("::"),
            ),
        ))
    } else {
        None
    }) else {
        return;
    };

    diagnostics.push(Diagnostic::policy(
        Some(path.to_path_buf()),
        Some(item_type.span().start().line),
        code,
        message,
    ));
}

fn analyze_glob_use_item(
    path: &Path,
    line: usize,
    is_reexport: bool,
    full_path: &[String],
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if is_reexport || full_path.is_empty() {
        return;
    }

    let Some(last_segment) = full_path.last() else {
        return;
    };
    if is_relative_keyword(last_segment) {
        return;
    }
    let starts_relative = full_path
        .first()
        .is_some_and(|segment| is_relative_keyword(segment));

    let rendered = format!("{}::*", full_path.join("::"));
    if last_segment.eq_ignore_ascii_case("prelude") {
        if starts_relative {
            return;
        }
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "namespace_prelude_glob_import",
            format!(
                "glob import `{rendered}` hides the real source modules at call sites; prefer explicit imports or keep the namespace visible"
            ),
        ));
        return;
    }

    if settings
        .namespace_preserving_modules
        .contains(&last_segment.to_ascii_lowercase())
    {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "namespace_glob_preserve_module",
            format!(
                "glob import `{rendered}` flattens configured namespace `{last_segment}`; prefer importing the namespace or the specific items you use"
            ),
        ));
    }
}

fn analyze_qualified_generic_path(
    file: &Path,
    path: &SynPath,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
    scope_use_bindings: &[ScopeUseBinding],
) {
    let full_path = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    let current_module_path = inferred_file_module_path(file);
    let rendered_path = full_path.join("::");

    if let Some(resolved_alias_path) = resolved_namespace_alias_path(&full_path, scope_use_bindings)
    {
        let preferred_path = preferred_qualified_path_surface(
            file,
            &current_module_path,
            &resolved_alias_path,
            settings,
        )
        .map(|candidate| candidate.rendered_path)
        .unwrap_or_else(|| resolved_alias_path.join("::"));
        if preferred_path != rendered_path {
            let diagnostic = Diagnostic::policy(
                Some(file.to_path_buf()),
                Some(path.span().start().line),
                "namespace_aliased_qualified_path",
                format!(
                    "`{rendered_path}` flattens a semantic module path; prefer `{preferred_path}`"
                ),
            );
            if !namespace_diagnostic_already_emitted(diagnostics, &diagnostic) {
                diagnostics.push(diagnostic);
            }
            return;
        }
    }

    let resolved_binding = resolved_namespace_binding(&full_path, scope_use_bindings);
    let analysis_path = resolved_binding
        .as_ref()
        .map(|binding| render_resolved_namespace_binding_path(&full_path, binding))
        .unwrap_or_else(|| full_path.clone());
    let Some(preferred_path) =
        preferred_qualified_path_surface(file, &current_module_path, &analysis_path, settings)
    else {
        return;
    };

    let diagnostic = if resolved_binding
        .as_ref()
        .is_some_and(|binding| binding.renamed)
    {
        Diagnostic::policy(
            Some(file.to_path_buf()),
            Some(path.span().start().line),
            "namespace_aliased_qualified_path",
            format!(
                "`{rendered_path}` flattens a semantic module path; prefer `{}`",
                preferred_path.rendered_path
            ),
        )
    } else {
        Diagnostic::policy(
            Some(file.to_path_buf()),
            Some(path.span().start().line),
            "namespace_redundant_qualified_generic",
            format!(
                "`{rendered_path}` repeats module context; prefer `{}`",
                preferred_path.rendered_path
            ),
        )
    };
    let diagnostic = if preferred_path.fixable {
        diagnostic.with_fix(replace_path_fix(preferred_path.rendered_path))
    } else {
        diagnostic
    };
    if !namespace_diagnostic_already_emitted(diagnostics, &diagnostic) {
        diagnostics.push(diagnostic);
    }
}

fn preferred_qualified_path_surface(
    file: &Path,
    current_module_path: &[String],
    full_path: &[String],
    settings: &NamespaceSettings,
) -> Option<QualifiedPathSurfaceCandidate> {
    let analysis_path = trim_relative_prefix(full_path);
    if analysis_path.len() < 2 {
        return None;
    }

    let parent_module = &analysis_path[analysis_path.len() - 2];
    let leaf_name = &analysis_path[analysis_path.len() - 1];
    let repeated_generic_context =
        qualified_generic_context_is_redundant(parent_module, leaf_name, settings);
    let repeated_module_context = path_context_is_redundant(parent_module, leaf_name);
    if !repeated_generic_context && !repeated_module_context {
        return None;
    }

    if let Some(preferred_path) =
        qualified_generic_parent_surface_candidate(file, current_module_path, full_path, leaf_name)
    {
        return Some(preferred_path);
    }

    if let Some(preferred_path) = promotable_parent_surface_candidate(
        file,
        current_module_path,
        full_path,
        leaf_name,
        settings,
    ) {
        return Some(preferred_path);
    }

    (repeated_generic_context && full_path.len() == 2).then(|| QualifiedPathSurfaceCandidate {
        rendered_path: leaf_name.to_string(),
        fixable: true,
    })
}

fn resolved_namespace_alias_path(
    full_path: &[String],
    scope_use_bindings: &[ScopeUseBinding],
) -> Option<Vec<String>> {
    let alias_name = full_path.first()?;
    let mut matches = scope_use_bindings.iter().filter(|binding| {
        binding.renamed
            && binding.binding.binding_name == *alias_name
            && actionable_namespace_alias(&binding.binding)
    });
    let binding = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    let mut resolved = binding.binding.full_path.clone();
    resolved.extend(full_path.iter().skip(1).cloned());
    Some(resolved)
}

fn resolved_namespace_binding(
    full_path: &[String],
    scope_use_bindings: &[ScopeUseBinding],
) -> Option<ScopeUseBinding> {
    let binding_name = full_path.first()?;
    let mut matches = scope_use_bindings.iter().filter(|binding| {
        binding.binding.binding_name == *binding_name
            && namespace_like_binding_name(&binding.binding.binding_name)
            && actionable_namespace_binding(binding)
    });
    let binding = matches.next()?.clone();
    if matches.next().is_some() {
        return None;
    }

    Some(binding)
}

fn render_resolved_namespace_binding_path(
    full_path: &[String],
    binding: &ScopeUseBinding,
) -> Vec<String> {
    let mut resolved = binding.binding.full_path.clone();
    resolved.extend(full_path.iter().skip(1).cloned());
    resolved
}

fn actionable_namespace_alias(binding: &UseBinding) -> bool {
    matches!(
        detect_name_style(&binding.source_name),
        super::NameStyle::Snake
    ) && matches!(
        detect_name_style(&binding.binding_name),
        super::NameStyle::Snake
    ) && alias_flattens_import_path(binding)
}

fn actionable_namespace_binding(binding: &ScopeUseBinding) -> bool {
    if !namespace_like_binding_name(&binding.binding.binding_name) {
        return false;
    }

    if binding.renamed {
        return binding.binding.full_path.len() >= 2;
    }

    binding.binding.full_path.len() >= 2
}

fn namespace_like_binding_name(binding_name: &str) -> bool {
    matches!(detect_name_style(binding_name), super::NameStyle::Snake)
}

fn alias_flattens_import_path(binding: &UseBinding) -> bool {
    if binding.full_path.len() < 2 {
        return false;
    }

    let source_segments = normalized_segments(&binding.source_name);
    let alias_segments = normalized_segments(&binding.binding_name);
    if source_segments.is_empty() || alias_segments == source_segments {
        return false;
    }

    let ancestor_segments = binding
        .full_path
        .iter()
        .take(binding.full_path.len() - 1)
        .flat_map(|segment| normalized_segments(segment))
        .collect::<BTreeSet<_>>();

    if alias_segments.starts_with(&source_segments) {
        let extra = &alias_segments[source_segments.len()..];
        return !extra.is_empty()
            && extra
                .iter()
                .all(|segment| ancestor_segments.contains(segment));
    }

    if alias_segments.ends_with(&source_segments) {
        let extra = &alias_segments[..alias_segments.len() - source_segments.len()];
        return !extra.is_empty()
            && extra
                .iter()
                .all(|segment| ancestor_segments.contains(segment));
    }

    false
}

fn normalized_segments(name: &str) -> Vec<String> {
    split_segments(name)
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect()
}

fn qualified_generic_parent_surface_candidate(
    path: &Path,
    current_module_path: &[String],
    full_path: &[String],
    leaf_name: &str,
) -> Option<QualifiedPathSurfaceCandidate> {
    if full_path.len() < 3 {
        return None;
    }

    for prefix_len in (1..=full_path.len() - 2).rev() {
        let parent_surface_prefix = &full_path[..prefix_len];
        let resolved_parent_surface =
            resolve_qualified_parent_surface_path(current_module_path, parent_surface_prefix)?;
        if !resolved_parent_surface.is_empty()
            && !resolved_parent_surface_adds_net_context(&resolved_parent_surface, leaf_name, None)
        {
            continue;
        }

        let descendant_path = &full_path[prefix_len..];
        if !module_publicly_reexports_descendant_leaf(
            path,
            &resolved_parent_surface,
            descendant_path,
        ) {
            continue;
        }

        return Some(QualifiedPathSurfaceCandidate {
            rendered_path: render_qualified_parent_surface(parent_surface_prefix, leaf_name),
            fixable: true,
        });
    }

    None
}

fn promotable_parent_surface_candidate(
    path: &Path,
    current_module_path: &[String],
    full_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> Option<QualifiedPathSurfaceCandidate> {
    if full_path.len() < 3 {
        return None;
    }

    for prefix_len in (1..=full_path.len() - 2).rev() {
        let parent_surface_prefix = &full_path[..prefix_len];
        let resolved_parent_surface =
            resolve_qualified_parent_surface_path(current_module_path, parent_surface_prefix)?;
        if !resolved_parent_surface_adds_net_context(
            &resolved_parent_surface,
            leaf_name,
            Some(settings),
        ) {
            continue;
        }

        let existing_bindings = module_bindings_for_module(path, &resolved_parent_surface);
        if existing_bindings.contains(leaf_name) {
            continue;
        }

        let Some(parent_module) = resolved_parent_surface.last() else {
            continue;
        };
        return Some(QualifiedPathSurfaceCandidate {
            rendered_path: format!("{parent_module}::{leaf_name}"),
            fixable: false,
        });
    }

    None
}

fn resolved_parent_surface_adds_net_context(
    resolved_parent_surface: &[String],
    leaf_name: &str,
    settings: Option<&NamespaceSettings>,
) -> bool {
    let Some(parent_module) = resolved_parent_surface.last() else {
        return false;
    };
    if path_context_is_redundant(parent_module, leaf_name) {
        return false;
    }

    if let Some(settings) = settings {
        let normalized_parent = parent_module.to_ascii_lowercase();
        if settings.weak_modules.contains(&normalized_parent)
            || settings.catch_all_modules.contains(&normalized_parent)
            || settings.organizational_modules.contains(&normalized_parent)
        {
            return false;
        }
    }

    true
}

fn resolve_qualified_parent_surface_path(
    current_module_path: &[String],
    path_prefix: &[String],
) -> Option<Vec<String>> {
    if path_prefix.is_empty() {
        return Some(Vec::new());
    }

    let mut base = current_module_path.to_vec();
    let mut iter = path_prefix.iter();
    let mut used_relative_prefix = false;

    while let Some(segment) = iter.next() {
        match segment.as_str() {
            "crate" => {
                base.clear();
                used_relative_prefix = true;
            }
            "self" => {
                used_relative_prefix = true;
            }
            "super" => {
                base.pop()?;
                used_relative_prefix = true;
            }
            other => {
                let mut resolved = if used_relative_prefix {
                    base
                } else {
                    Vec::new()
                };
                resolved.push(other.to_string());
                resolved.extend(iter.cloned());
                return Some(resolved);
            }
        }
    }

    Some(base)
}

fn render_qualified_parent_surface(path_prefix: &[String], leaf_name: &str) -> String {
    if path_prefix.is_empty() {
        return leaf_name.to_string();
    }

    format!("{}::{leaf_name}", path_prefix.join("::"))
}

fn binding_is_used_as_namespace_in_scope(scope_items: &[Item], binding_name: &str) -> bool {
    let mut visitor = BindingNamespaceUseVisitor {
        binding_name,
        used_as_namespace: false,
    };

    for item in scope_items {
        match item {
            Item::Use(_) | Item::Mod(_) => continue,
            other => {
                visitor.visit_item(other);
                if visitor.used_as_namespace {
                    return true;
                }
            }
        }
    }

    false
}

struct BindingNamespaceUseVisitor<'a> {
    binding_name: &'a str,
    used_as_namespace: bool,
}

impl<'ast> Visit<'ast> for BindingNamespaceUseVisitor<'_> {
    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if path_uses_binding_as_namespace(&node.path, self.binding_name) {
            self.used_as_namespace = true;
            return;
        }
        visit::visit_expr_path(self, node);
    }

    fn visit_type_path(&mut self, node: &'ast TypePath) {
        if node.qself.is_none() && path_uses_binding_as_namespace(&node.path, self.binding_name) {
            self.used_as_namespace = true;
            return;
        }
        visit::visit_type_path(self, node);
    }

    fn visit_item_mod(&mut self, _node: &'ast ItemMod) {}
}

fn path_uses_binding_as_namespace(path: &SynPath, binding_name: &str) -> bool {
    path.segments.len() >= 2
        && path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == binding_name)
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
            leaves.push(UseLeaf::Direct(UseBinding {
                full_path,
                source_name: source_name.clone(),
                binding_name: source_name,
            }));
        }
        UseTree::Rename(rename) => {
            let mut full_path = prefix;
            let source_name = rename.ident.to_string();
            full_path.push(source_name.clone());
            leaves.push(UseLeaf::Rename(UseBinding {
                full_path,
                source_name,
                binding_name: rename.rename.to_string(),
            }));
        }
        UseTree::Glob(_) => leaves.push(UseLeaf::Glob(prefix)),
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(prefix.clone(), item, leaves);
            }
        }
    }
}

fn collect_scope_use_bindings(items: &[Item]) -> Vec<ScopeUseBinding> {
    let mut bindings = Vec::new();

    for item in items {
        let Item::Use(item_use) = item else {
            continue;
        };
        let mut leaves = Vec::new();
        flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
        for leaf in leaves {
            match leaf {
                UseLeaf::Direct(binding) => bindings.push(ScopeUseBinding {
                    binding,
                    renamed: false,
                }),
                UseLeaf::Rename(binding) => bindings.push(ScopeUseBinding {
                    binding,
                    renamed: true,
                }),
                UseLeaf::Glob(_) => {}
            }
        }
    }

    bindings
}

fn generic_noun_message(
    is_reexport: bool,
    source_name: &str,
    visible_callsite_surface: &str,
) -> (&'static str, String) {
    if is_reexport {
        (
            "namespace_flat_pub_use",
            format!(
                "flattened re-export hides namespace context for `{source_name}`; prefer `{visible_callsite_surface}`"
            ),
        )
    } else {
        (
            "namespace_flat_use",
            format!(
                "flattened import hides namespace context for `{source_name}`; prefer `{visible_callsite_surface}`"
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
                "flattened re-export keeps redundant `{parent_module}` context in `{binding_name}`; prefer `{parent_module}::{shorter_leaf}`"
            ),
        )
    } else {
        (
            "namespace_flat_use_redundant_leaf_context",
            format!(
                "flattened import keeps redundant `{parent_module}` context in `{binding_name}`; prefer `{parent_module}::{shorter_leaf}`"
            ),
        )
    }
}

fn preserve_module_message(
    is_reexport: bool,
    source_name: &str,
    binding_name: &str,
    visible_callsite_surface: &str,
) -> (&'static str, String) {
    if is_reexport {
        (
            "namespace_flat_pub_use_preserve_module",
            format!(
                "flattened re-export hides configured namespace context for `{source_name}`; prefer `{visible_callsite_surface}`"
            ),
        )
    } else {
        (
            "namespace_flat_use_preserve_module",
            format!(
                "flattened import hides configured namespace context for `{binding_name}`; prefer `{visible_callsite_surface}`"
            ),
        )
    }
}

fn visible_callsite_surface_candidate(
    import_path: &[String],
    binding_name: &str,
    settings: &NamespaceSettings,
) -> Option<String> {
    let parent_module = import_path.iter().rev().nth(1)?;
    if qualified_generic_context_is_redundant(parent_module, binding_name, settings) {
        return None;
    }

    Some(format!("{parent_module}::{binding_name}"))
}

fn redundant_leaf_context_candidate(
    full_path: &[String],
    leaf_name: &str,
    renamed: bool,
    is_reexport: bool,
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
            if prefix_overlap_is_actionable(
                full_path,
                renamed,
                &shorter_leaf,
                is_reexport,
                settings,
            ) {
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
    renamed: bool,
    shorter_leaf: &str,
    is_reexport: bool,
    settings: &NamespaceSettings,
) -> bool {
    if is_unreadable_short_leaf(shorter_leaf) {
        return false;
    }

    if renamed {
        if is_reexport {
            return true;
        }

        return rename_overlap_is_actionable(full_path, shorter_leaf, settings);
    }

    if is_reexport {
        return full_path.len() <= 3;
    }

    full_path.len() <= 3
        && split_segments(shorter_leaf)
            .last()
            .is_some_and(|segment| matches_generic_noun(segment, settings))
}

fn rename_overlap_is_actionable(
    full_path: &[String],
    shorter_leaf: &str,
    settings: &NamespaceSettings,
) -> bool {
    if full_path.len() > 2 {
        return true;
    }

    let Some(parent_module) = full_path.iter().rev().nth(1) else {
        return false;
    };

    settings
        .namespace_preserving_modules
        .contains(&parent_module.to_ascii_lowercase())
        || split_segments(shorter_leaf)
            .last()
            .is_some_and(|segment| matches_generic_noun(segment, settings))
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

fn qualified_generic_context_is_redundant(
    parent_module: &str,
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> bool {
    path_context_is_redundant(parent_module, leaf_name) && matches_generic_noun(leaf_name, settings)
}

fn path_context_is_redundant(parent_module: &str, leaf_name: &str) -> bool {
    normalized_segments(parent_module) == normalized_segments(leaf_name)
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
            "`{parent_module}::{source_name}` bypasses the canonical parent surface for `{binding_name}`; prefer `{canonical_parent_surface}`"
        ),
    )
}

fn public_bindings_for_module(path: &Path, module_path: &[String]) -> BTreeSet<String> {
    bindings_for_module(path, module_path, true)
}

fn module_bindings_for_module(path: &Path, module_path: &[String]) -> BTreeSet<String> {
    bindings_for_module(path, module_path, false)
}

fn bindings_for_module(path: &Path, module_path: &[String], public_only: bool) -> BTreeSet<String> {
    if let Some(bindings) = inline_module_bindings_for_module(path, module_path, public_only) {
        return bindings;
    }

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
        return collect_bindings(&parsed.items, public_only);
    }

    BTreeSet::new()
}

fn module_publicly_reexports_descendant_leaf(
    path: &Path,
    module_path: &[String],
    descendant_path: &[String],
) -> bool {
    if descendant_path.is_empty() {
        return false;
    }

    if let Some(items) = inline_module_items_for_module(path, module_path) {
        return items_publicly_reexport_descendant_leaf(&items, module_path, descendant_path);
    }

    let Some(src_root) = source_root(path) else {
        return false;
    };

    for candidate in parent_module_files(&src_root, module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        return items_publicly_reexport_descendant_leaf(
            &parsed.items,
            module_path,
            descendant_path,
        );
    }

    false
}

fn items_publicly_reexport_descendant_leaf(
    items: &[Item],
    module_path: &[String],
    descendant_path: &[String],
) -> bool {
    let expected_path = module_path
        .iter()
        .cloned()
        .chain(descendant_path.iter().cloned())
        .collect::<Vec<_>>();
    let expected_leaf = descendant_path
        .last()
        .expect("descendant path is non-empty");

    items.iter().any(|item| {
        let Item::Use(item_use) = item else {
            return false;
        };
        if matches!(item_use.vis, Visibility::Inherited) {
            return false;
        }

        let mut leaves = Vec::new();
        flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
        leaves.into_iter().any(|leaf| {
            let binding = match leaf {
                UseLeaf::Direct(binding) | UseLeaf::Rename(binding) => binding,
                UseLeaf::Glob(_) => return false,
            };
            if &binding.binding_name != expected_leaf {
                return false;
            }

            let resolved_prefix = resolve_qualified_parent_surface_path(
                module_path,
                &binding.full_path[..binding.full_path.len() - 1],
            );
            let Some(mut resolved_path) = resolved_prefix else {
                return false;
            };
            resolved_path.push(binding.source_name);
            resolved_path == expected_path
        })
    })
}

fn inline_module_bindings_for_module(
    path: &Path,
    module_path: &[String],
    public_only: bool,
) -> Option<BTreeSet<String>> {
    let current_module_path = inferred_file_module_path(path);
    if module_path.len() < current_module_path.len()
        || !module_path.starts_with(&current_module_path)
    {
        return None;
    }

    let src = fs::read_to_string(path).ok()?;
    let parsed = syn::parse_file(&src).ok()?;
    let nested_path = &module_path[current_module_path.len()..];
    let items = nested_inline_module_items(&parsed.items, nested_path)?;
    Some(collect_bindings(items, public_only))
}

fn nested_inline_module_items<'a>(items: &'a [Item], path: &[String]) -> Option<&'a [Item]> {
    let Some((head, tail)) = path.split_first() else {
        return Some(items);
    };

    let module = items.iter().find_map(|item| {
        let Item::Mod(item_mod) = item else {
            return None;
        };
        (item_mod.ident == head.as_str()).then_some(item_mod)
    })?;
    let (_, nested) = module.content.as_ref()?;
    nested_inline_module_items(nested, tail)
}

fn inline_module_items_for_module(path: &Path, module_path: &[String]) -> Option<Vec<Item>> {
    let current_module_path = inferred_file_module_path(path);
    if module_path.len() < current_module_path.len()
        || !module_path.starts_with(&current_module_path)
    {
        return None;
    }

    let src = fs::read_to_string(path).ok()?;
    let parsed = syn::parse_file(&src).ok()?;
    let nested_path = &module_path[current_module_path.len()..];
    let items = nested_inline_module_items(&parsed.items, nested_path)?;
    Some(items.to_vec())
}

fn collect_bindings(items: &[Item], public_only: bool) -> BTreeSet<String> {
    let mut bindings = BTreeSet::new();

    for item in items {
        match item {
            Item::Use(item_use)
                if !public_only || !matches!(item_use.vis, Visibility::Inherited) =>
            {
                let mut leaves = Vec::new();
                flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
                for leaf in leaves {
                    let binding = match leaf {
                        UseLeaf::Direct(binding) | UseLeaf::Rename(binding) => binding,
                        UseLeaf::Glob(_) => continue,
                    };
                    if !is_nonbinding_import(&binding.binding_name) {
                        bindings.insert(binding.binding_name);
                    }
                }
            }
            _ => {
                if let Some((binding_name, is_public)) = public_item_binding(item)
                    && (!public_only || is_public)
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
    _path: &Path,
    module_path: &[String],
    binding_name: &str,
) -> String {
    if module_path.is_empty() {
        return format!("crate::{binding_name}");
    }

    format!("{}::{binding_name}", module_path.join("::"))
}

fn namespace_diagnostic_already_emitted(
    diagnostics: &[Diagnostic],
    candidate: &Diagnostic,
) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.code() == candidate.code()
            && diagnostic.file == candidate.file
            && diagnostic.line == candidate.line
            && diagnostic.message == candidate.message
            && diagnostic.fix == candidate.fix
    })
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
