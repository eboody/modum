use std::{collections::BTreeSet, fs, path::Path};

use syn::{
    ExprPath, File, Item, ItemConst, ItemEnum, ItemFn, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemTraitAlias, ItemType, ItemUnion, ItemUse, Path as SynPath, TypeParamBound, TypePath,
    UseTree, Visibility,
    spanned::Spanned,
    visit::{self, Visit},
};

use super::{
    Diagnostic, NamespaceSettings, detect_name_style, inferred_file_module_path, is_public,
    normalize_segment, parent_module_files, render_segments, replace_path_fix, source_root,
    split_segments,
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

#[derive(Clone)]
struct PendingUnsupportedUse {
    source_name: String,
    constructs: BTreeSet<String>,
}

#[derive(Clone, Copy)]
enum UnsupportedNamespaceFamilyAction {
    Import,
    ReExport,
    Alias,
}

#[derive(Clone, PartialEq, Eq)]
struct QualifiedPathSurfaceCandidate {
    rendered_path: String,
    fixable: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct OverqualifiedCallsiteCandidate {
    action: OverqualifiedCallsiteAction,
    rendered_path: String,
}

#[derive(Clone, PartialEq, Eq)]
enum OverqualifiedCallsiteAction {
    ImportParent { import_path: String },
    UseExistingBinding { binding_name: String },
}

struct NamespaceFamilyBindingInfo {
    bindings: BTreeSet<String>,
    unsupported_constructs: BTreeSet<String>,
}

#[derive(Clone)]
struct ErrorSurfaceFollowThroughCandidate {
    preferred_path: String,
    target_exists: bool,
}

enum NamespaceVisibilityDependency {
    Visible,
    Unsupported(BTreeSet<String>),
}

pub(super) fn analyze_namespace_rules(
    path: &Path,
    parsed: &File,
    settings: &NamespaceSettings,
) -> Analysis {
    let mut diagnostics = Vec::new();
    analyze_scope(
        path,
        &parsed.items,
        &inferred_file_module_path(path),
        settings,
        &mut diagnostics,
    );
    Analysis { diagnostics }
}

fn analyze_scope(
    path: &Path,
    items: &[Item],
    current_module_path: &[String],
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope_use_bindings = collect_scope_use_bindings(items);

    for item in items {
        match item {
            Item::Use(item_use) => analyze_use_item(
                path,
                current_module_path,
                items,
                item_use,
                settings,
                diagnostics,
            ),
            Item::Type(item_type) => {
                analyze_type_alias_item(
                    path,
                    current_module_path,
                    item_type,
                    settings,
                    diagnostics,
                );
                analyze_qualified_callsite_paths(
                    path,
                    current_module_path,
                    item,
                    settings,
                    diagnostics,
                    &scope_use_bindings,
                );
            }
            Item::Mod(ItemMod {
                content: Some((_, nested)),
                ident,
                ..
            }) => {
                let mut nested_module_path = current_module_path.to_vec();
                nested_module_path.push(ident.to_string());
                analyze_scope(path, nested, &nested_module_path, settings, diagnostics);
            }
            _ => analyze_qualified_callsite_paths(
                path,
                current_module_path,
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
    current_module_path: &[String],
    item: &Item,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
    scope_use_bindings: &[ScopeUseBinding],
) {
    let mut visitor = QualifiedCallsitePathVisitor {
        file: path,
        current_module_path,
        settings,
        diagnostics,
        scope_use_bindings,
    };
    visitor.visit_item(item);
}

struct QualifiedCallsitePathVisitor<'a> {
    file: &'a Path,
    current_module_path: &'a [String],
    settings: &'a NamespaceSettings,
    diagnostics: &'a mut Vec<Diagnostic>,
    scope_use_bindings: &'a [ScopeUseBinding],
}

impl<'ast> Visit<'ast> for QualifiedCallsitePathVisitor<'_> {
    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        analyze_qualified_generic_path(
            self.file,
            self.current_module_path,
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
                self.current_module_path,
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
                self.current_module_path,
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
    current_module_path: &[String],
    scope_items: &[Item],
    item_use: &ItemUse,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut leaves = Vec::new();
    let mut pending_unsupported = Vec::<PendingUnsupportedUse>::new();
    flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
    let line = item_use.span().start().line;
    let is_reexport = !matches!(item_use.vis, Visibility::Inherited);
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
        let Some(source_leaf_name) = analysis_path.last() else {
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
                    current_module_path,
                    analysis_path,
                    &binding.binding_name,
                    settings,
                )
                || parent_surface_reexports_current_binding(
                    path,
                    current_module_path,
                    &binding.binding_name,
                )
                || preserved_parent_surface_reexport(current_module_path, analysis_path, settings));

        if skip_reexport {
            continue;
        }

        let canonical_parent_surface = if is_reexport {
            None
        } else {
            canonical_parent_surface_candidate(
                path,
                current_module_path,
                analysis_path,
                &binding.binding_name,
                settings,
            )
        };
        let visible_callsite_surface =
            visible_callsite_surface_candidate(analysis_path, source_leaf_name, settings);
        let namespace_visibility_dependency = namespace_visibility_dependency(
            path,
            current_module_path,
            &binding.full_path,
            source_leaf_name,
            settings,
        );
        let binding_used_as_namespace =
            binding_is_used_as_namespace_in_scope(scope_items, &binding.binding_name);
        let error_surface_follow_through = error_surface_follow_through_candidate(
            path,
            current_module_path,
            &binding.full_path,
            settings,
            &[],
        );

        let (code, message) =
            if let Some(error_surface_follow_through) = error_surface_follow_through.as_ref() {
                error_surface_follow_through_use_message(
                    is_reexport,
                    &binding.full_path.join("::"),
                    &error_surface_follow_through.preferred_path,
                    error_surface_follow_through.target_exists,
                )
            } else if let Some(canonical_parent_surface) = canonical_parent_surface.as_deref() {
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
            } else if (settings.generic_nouns.contains(source_leaf_name)
                || matches!(
                    namespace_visibility_dependency,
                    Some(NamespaceVisibilityDependency::Visible)
                ))
                && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
            {
                generic_noun_message(is_reexport, &binding.source_name, visible_callsite_surface)
            } else if settings
                .namespace_preserving_modules
                .contains(&parent_normalized)
                && !module_path_contains_namespace(current_module_path, &parent_normalized)
                && !binding_used_as_namespace
                && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
            {
                preserve_module_message(
                    is_reexport,
                    &binding.source_name,
                    &binding.binding_name,
                    visible_callsite_surface,
                )
            } else if let Some(NamespaceVisibilityDependency::Unsupported(constructs)) =
                namespace_visibility_dependency.as_ref()
            {
                pending_unsupported.push(PendingUnsupportedUse {
                    source_name: binding.source_name.clone(),
                    constructs: constructs.clone(),
                });
                continue;
            } else {
                continue;
            };

        let diagnostic = if code == "namespace_family_unsupported_construct"
            || (code.starts_with("namespace_")
                && code.contains("error_surface_follow_through")
                && error_surface_follow_through
                    .as_ref()
                    .is_some_and(|candidate| !candidate.target_exists))
        {
            Diagnostic::advisory(Some(path.to_path_buf()), Some(line), code, message)
        } else {
            Diagnostic::policy(Some(path.to_path_buf()), Some(line), code, message)
        };
        diagnostics.push(diagnostic);
    }

    for grouped in group_pending_unsupported_uses(&pending_unsupported) {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "namespace_family_unsupported_construct",
            namespace_family_unsupported_construct_message(
                if is_reexport {
                    UnsupportedNamespaceFamilyAction::ReExport
                } else {
                    UnsupportedNamespaceFamilyAction::Import
                },
                &grouped.source_names,
                &grouped.constructs,
            )
            .1,
        ));
    }
}

fn analyze_type_alias_item(
    path: &Path,
    current_module_path: &[String],
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
    let Some(aliased_leaf_name) = analysis_path.last() else {
        return;
    };
    let parent_normalized = parent_module.to_ascii_lowercase();
    let redundant_leaf =
        redundant_leaf_context_candidate(analysis_path, &binding_name, true, false, settings);
    let visible_callsite_surface =
        visible_callsite_surface_candidate(analysis_path, aliased_leaf_name, settings);
    let error_surface_follow_through = error_surface_follow_through_candidate(
        path,
        current_module_path,
        &full_path,
        settings,
        &[],
    );
    let namespace_visibility_dependency = namespace_visibility_dependency(
        path,
        current_module_path,
        &full_path,
        aliased_leaf_name,
        settings,
    );

    let Some((code, message)) = (if let Some(error_surface_follow_through) =
        error_surface_follow_through.as_ref()
    {
        Some(error_surface_follow_through_alias_message(
            &binding_name,
            &full_path.join("::"),
            &error_surface_follow_through.preferred_path,
            error_surface_follow_through.target_exists,
        ))
    } else if let Some(shorter_leaf) = redundant_leaf {
        Some((
            "namespace_flat_type_alias_redundant_leaf_context",
            format!(
                "type alias `{binding_name}` keeps redundant `{parent_module}` context; prefer `{parent_module}::{shorter_leaf}` directly"
            ),
        ))
    } else if (settings.generic_nouns.contains(aliased_leaf_name)
        || matches!(
            namespace_visibility_dependency,
            Some(NamespaceVisibilityDependency::Visible)
        ))
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
        && !module_path_contains_namespace(current_module_path, &parent_normalized)
        && let Some(visible_callsite_surface) = visible_callsite_surface.as_deref()
    {
        Some((
            "namespace_flat_type_alias_preserve_module",
            format!(
                "type alias `{binding_name}` hides configured namespace context for `{}`; prefer `{visible_callsite_surface}` directly",
                full_path.join("::"),
            ),
        ))
    } else if let Some(NamespaceVisibilityDependency::Unsupported(constructs)) =
        namespace_visibility_dependency.as_ref()
    {
        let rendered_path = full_path.join("::");
        Some(namespace_family_unsupported_construct_message(
            UnsupportedNamespaceFamilyAction::Alias,
            &[rendered_path],
            constructs,
        ))
    } else {
        None
    }) else {
        return;
    };

    let diagnostic = if code == "namespace_family_unsupported_construct"
        || (code == "namespace_flat_type_alias_error_surface_follow_through"
            && error_surface_follow_through
                .as_ref()
                .is_some_and(|candidate| !candidate.target_exists))
    {
        Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(item_type.span().start().line),
            code,
            message,
        )
    } else {
        Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(item_type.span().start().line),
            code,
            message,
        )
    };
    diagnostics.push(diagnostic);
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
    current_module_path: &[String],
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
    let rendered_path = full_path.join("::");

    if let Some(resolved_alias_path) = resolved_namespace_alias_path(&full_path, scope_use_bindings)
    {
        let preferred_path = preferred_qualified_path_surface(
            file,
            current_module_path,
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
    if let Some(error_surface_follow_through) = error_surface_follow_through_candidate(
        file,
        current_module_path,
        &analysis_path,
        settings,
        scope_use_bindings,
    ) {
        let diagnostic = if error_surface_follow_through.target_exists {
            Diagnostic::policy(
                Some(file.to_path_buf()),
                Some(path.span().start().line),
                "namespace_qualified_error_surface_follow_through",
                format!(
                    "`{rendered_path}` keeps a flatter error surface than the owning path; prefer `{}`",
                    error_surface_follow_through.preferred_path
                ),
            )
        } else {
            Diagnostic::advisory(
                Some(file.to_path_buf()),
                Some(path.span().start().line),
                "namespace_qualified_error_surface_follow_through",
                format!(
                    "`{rendered_path}` keeps a flat leaf error surface; prefer `{}` once that facet exists",
                    error_surface_follow_through.preferred_path
                ),
            )
        };
        if !namespace_diagnostic_already_emitted(diagnostics, &diagnostic) {
            diagnostics.push(diagnostic);
        }
        return;
    }
    let Some(preferred_path) =
        preferred_qualified_path_surface(file, current_module_path, &analysis_path, settings)
    else {
        if let Some(preferred_path) = preferred_overqualified_callsite_candidate(
            existing_namespace_callsite_candidate(&full_path, settings, scope_use_bindings),
            overqualified_callsite_candidate(file, current_module_path, &full_path, settings),
        ) {
            let message = match &preferred_path.action {
                OverqualifiedCallsiteAction::ImportParent { import_path } => format!(
                    "`{rendered_path}` keeps too much module scaffolding at the call site; import `{import_path}` and prefer `{}`",
                    preferred_path.rendered_path
                ),
                OverqualifiedCallsiteAction::UseExistingBinding { binding_name } => format!(
                    "`{rendered_path}` keeps too much module scaffolding at the call site; call through existing `{binding_name}` namespace and prefer `{}`",
                    preferred_path.rendered_path
                ),
            };
            let diagnostic = Diagnostic::advisory(
                Some(file.to_path_buf()),
                Some(path.span().start().line),
                "namespace_overqualified_callsite_path",
                message,
            );
            if !namespace_diagnostic_already_emitted(diagnostics, &diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
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

fn error_surface_follow_through_use_message(
    is_reexport: bool,
    source_path: &str,
    preferred_path: &str,
    target_exists: bool,
) -> (&'static str, String) {
    let code = if is_reexport {
        "namespace_flat_pub_use_error_surface_follow_through"
    } else {
        "namespace_flat_use_error_surface_follow_through"
    };
    let subject = if is_reexport {
        "flattened re-export"
    } else {
        "flattened import"
    };
    let timing = if target_exists {
        String::new()
    } else {
        " once that facet exists".to_string()
    };
    (
        code,
        format!(
            "{subject} keeps flat error surface `{source_path}`; prefer `{preferred_path}`{timing}"
        ),
    )
}

fn error_surface_follow_through_alias_message(
    binding_name: &str,
    source_path: &str,
    preferred_path: &str,
    target_exists: bool,
) -> (&'static str, String) {
    let timing = if target_exists {
        String::new()
    } else {
        " once that facet exists".to_string()
    };
    (
        "namespace_flat_type_alias_error_surface_follow_through",
        format!(
            "type alias `{binding_name}` keeps flat error surface `{source_path}`; prefer `{preferred_path}`{timing}"
        ),
    )
}

fn preferred_overqualified_callsite_candidate(
    existing: Option<OverqualifiedCallsiteCandidate>,
    imported: Option<OverqualifiedCallsiteCandidate>,
) -> Option<OverqualifiedCallsiteCandidate> {
    match (existing, imported) {
        (Some(existing), Some(imported)) => {
            let existing_score = overqualified_callsite_score(&existing);
            let imported_score = overqualified_callsite_score(&imported);
            if existing_score < imported_score {
                Some(existing)
            } else if imported_score < existing_score {
                Some(imported)
            } else if matches!(
                existing.action,
                OverqualifiedCallsiteAction::UseExistingBinding { .. }
            ) {
                Some(existing)
            } else {
                Some(imported)
            }
        }
        (Some(existing), None) => Some(existing),
        (None, Some(imported)) => Some(imported),
        (None, None) => None,
    }
}

fn overqualified_callsite_score(candidate: &OverqualifiedCallsiteCandidate) -> (usize, usize) {
    (
        candidate.rendered_path.matches("::").count(),
        candidate.rendered_path.len(),
    )
}

fn overqualified_callsite_candidate(
    file: &Path,
    _current_module_path: &[String],
    full_path: &[String],
    settings: &NamespaceSettings,
) -> Option<OverqualifiedCallsiteCandidate> {
    let analysis_path = trim_relative_prefix(full_path);
    if analysis_path.len() < 4 {
        return None;
    }

    let rendered_path = full_path.join("::");
    if full_path.len() < 5 && rendered_path.len() < 32 {
        return None;
    }

    if !qualified_path_is_owned_or_local(file, full_path, analysis_path, settings) {
        return None;
    }

    let module_count = full_path.len().saturating_sub(1);
    let leaf_name = full_path.last()?;
    for visible_suffix_len in 1..module_count {
        let import_path = &full_path[..full_path.len() - visible_suffix_len];
        let visible_modules =
            &full_path[full_path.len() - 1 - visible_suffix_len..full_path.len() - 1];
        if !callsite_visible_suffix_is_meaningful(
            visible_modules,
            leaf_name,
            module_count,
            settings,
        ) {
            continue;
        }

        return Some(OverqualifiedCallsiteCandidate {
            action: OverqualifiedCallsiteAction::ImportParent {
                import_path: import_path.join("::"),
            },
            rendered_path: format!("{}::{leaf_name}", visible_modules.join("::")),
        });
    }

    None
}

fn existing_namespace_callsite_candidate(
    full_path: &[String],
    settings: &NamespaceSettings,
    scope_use_bindings: &[ScopeUseBinding],
) -> Option<OverqualifiedCallsiteCandidate> {
    let leaf_name = full_path.last()?;
    let total_parent_modules = full_path.len().saturating_sub(1);

    scope_use_bindings
        .iter()
        .filter(|binding| callsite_actionable_namespace_binding(binding))
        .filter_map(|binding| {
            let binding_path = &binding.binding.full_path;
            if binding_path.len() < 2
                || binding_path.len() >= full_path.len()
                || !full_path.starts_with(binding_path)
            {
                return None;
            }

            let mut visible_modules = vec![binding.binding.binding_name.clone()];
            visible_modules.extend(
                full_path[binding_path.len()..full_path.len() - 1]
                    .iter()
                    .cloned(),
            );
            if !callsite_visible_suffix_is_meaningful(
                &visible_modules,
                leaf_name,
                total_parent_modules,
                settings,
            ) {
                return None;
            }

            Some(OverqualifiedCallsiteCandidate {
                action: OverqualifiedCallsiteAction::UseExistingBinding {
                    binding_name: binding.binding.binding_name.clone(),
                },
                rendered_path: format!("{}::{leaf_name}", visible_modules.join("::")),
            })
        })
        .min_by_key(|candidate| {
            (
                candidate.rendered_path.matches("::").count(),
                candidate.rendered_path.len(),
            )
        })
}

fn callsite_actionable_namespace_binding(binding: &ScopeUseBinding) -> bool {
    if !callsite_parent_module_looks_like_namespace(&binding.binding.binding_name) {
        return false;
    }

    binding.binding.full_path.len() >= 2
}

fn callsite_visible_suffix_is_meaningful(
    visible_modules: &[String],
    leaf_name: &str,
    total_parent_modules: usize,
    settings: &NamespaceSettings,
) -> bool {
    let Some(parent_module) = visible_modules.last() else {
        return false;
    };
    if !callsite_parent_module_looks_like_namespace(parent_module) {
        return false;
    }
    if !resolved_parent_surface_adds_net_context(visible_modules, leaf_name, Some(settings)) {
        return false;
    }
    if visible_modules.len() == 1
        && matches_generic_noun(leaf_name, settings)
        && total_parent_modules > 2
        && !settings
            .namespace_preserving_modules
            .contains(&parent_module.to_ascii_lowercase())
    {
        return false;
    }

    true
}

fn callsite_parent_module_looks_like_namespace(parent_module: &str) -> bool {
    namespace_like_binding_name(parent_module)
        || parent_module
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn qualified_path_is_owned_or_local(
    file: &Path,
    full_path: &[String],
    analysis_path: &[String],
    settings: &NamespaceSettings,
) -> bool {
    full_path
        .first()
        .is_some_and(|segment| is_relative_keyword(segment))
        || full_path
            .first()
            .is_some_and(|segment| settings.owned_crate_names.contains(segment))
        || module_path_exists_in_current_crate(
            file,
            &analysis_path[..analysis_path.len().saturating_sub(1)],
        )
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
        if !promotable_parent_surface_is_owned(path, full_path, &resolved_parent_surface, settings)
        {
            continue;
        }
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

fn promotable_parent_surface_is_owned(
    path: &Path,
    full_path: &[String],
    resolved_parent_surface: &[String],
    settings: &NamespaceSettings,
) -> bool {
    full_path
        .first()
        .is_some_and(|segment| is_relative_keyword(segment))
        || module_path_exists_in_current_crate(path, resolved_parent_surface)
        || full_path
            .first()
            .is_some_and(|segment| settings.owned_crate_names.contains(segment))
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

fn namespace_visibility_dependency(
    path: &Path,
    current_module_path: &[String],
    import_path: &[String],
    binding_name: &str,
    settings: &NamespaceSettings,
) -> Option<NamespaceVisibilityDependency> {
    let shared_prefix = namespace_family_shared_prefix(binding_name, settings)?;
    let path_prefix = import_path.get(..import_path.len().saturating_sub(1))?;
    let module_path = resolve_qualified_parent_surface_path(current_module_path, path_prefix)?;
    if module_path.is_empty() {
        return None;
    }

    let binding_info = namespace_family_binding_info_for_module(path, &module_path);
    let has_visible_witness = binding_info.bindings.iter().any(|candidate| {
        namespace_family_shared_prefix(candidate, settings).is_some_and(|candidate_prefix| {
            candidate != binding_name && candidate_prefix == shared_prefix
        })
    });

    if has_visible_witness {
        return Some(NamespaceVisibilityDependency::Visible);
    }

    (!binding_info.unsupported_constructs.is_empty()).then_some(
        NamespaceVisibilityDependency::Unsupported(binding_info.unsupported_constructs),
    )
}

fn error_surface_follow_through_candidate(
    path: &Path,
    current_module_path: &[String],
    full_path: &[String],
    settings: &NamespaceSettings,
    scope_use_bindings: &[ScopeUseBinding],
) -> Option<ErrorSurfaceFollowThroughCandidate> {
    let analysis_path = trim_relative_prefix(full_path);
    if analysis_path.len() < 2 {
        return None;
    }

    let leaf_name = analysis_path.last()?;
    let import_prefix = full_path.get(..full_path.len().saturating_sub(1))?;
    let module_path = resolve_qualified_parent_surface_path(current_module_path, import_prefix)?;
    if module_path.is_empty() {
        return None;
    }

    let (preferred_absolute_path, target_exists) =
        error_surface_follow_through_target(path, &module_path, leaf_name, settings)?;
    let preferred_path = preferred_error_surface_visible_path(
        path,
        current_module_path,
        &preferred_absolute_path,
        settings,
        scope_use_bindings,
    );

    Some(ErrorSurfaceFollowThroughCandidate {
        preferred_path,
        target_exists,
    })
}

fn preferred_error_surface_visible_path(
    file: &Path,
    current_module_path: &[String],
    absolute_target_path: &[String],
    settings: &NamespaceSettings,
    scope_use_bindings: &[ScopeUseBinding],
) -> String {
    preferred_overqualified_callsite_candidate(
        existing_namespace_callsite_candidate(absolute_target_path, settings, scope_use_bindings),
        overqualified_callsite_candidate(file, current_module_path, absolute_target_path, settings),
    )
    .map(|candidate| candidate.rendered_path)
    .unwrap_or_else(|| absolute_target_path.join("::"))
}

fn error_surface_follow_through_target(
    path: &Path,
    module_path: &[String],
    flat_error_name: &str,
    settings: &NamespaceSettings,
) -> Option<(Vec<String>, bool)> {
    if flat_error_name == "Error" || !flat_error_name.ends_with("Error") {
        return None;
    }

    let items = module_items_for_module(path, module_path, settings)?;
    if let Some(target_path) =
        owned_facet_error_surface_target(&items, module_path, flat_error_name)
    {
        return Some((target_path, true));
    }

    if let Some((child_module_name, leaf_name)) =
        candidate_child_facet_module_target(&items, module_path, settings)
        && flat_error_name
            .strip_suffix("Error")
            .is_some_and(|companion_leaf| companion_leaf == leaf_name)
    {
        let mut child_module_path = module_path.to_vec();
        child_module_path.push(child_module_name);
        let mut preferred_path = child_module_path.clone();
        preferred_path.push("Error".to_string());
        let target_exists =
            public_bindings_for_owned_module(path, &child_module_path, settings).contains("Error");
        return Some((preferred_path, target_exists));
    }

    let root_module_path = boundary_owner_module_path(module_path);
    if root_module_path != module_path {
        return None;
    }

    let facet_module_name =
        candidate_root_facet_module_target(path, &items, module_path, flat_error_name, settings)?;
    let mut facet_module_path = module_path.to_vec();
    facet_module_path.push(facet_module_name);
    let mut preferred_path = facet_module_path.clone();
    preferred_path.push("Error".to_string());
    let target_exists =
        public_bindings_for_owned_module(path, &facet_module_path, settings).contains("Error");
    Some((preferred_path, target_exists))
}

fn module_items_for_module(
    path: &Path,
    module_path: &[String],
    settings: &NamespaceSettings,
) -> Option<Vec<Item>> {
    if !module_path_starts_with_owned_crate(module_path, settings)
        && let Some(items) = inline_module_items_for_module(path, module_path)
    {
        return Some(items);
    }

    let (src_root, relative_module_path) =
        source_root_and_relative_module_path(path, module_path, settings)?;

    for candidate in parent_module_files(&src_root, &relative_module_path) {
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

fn public_bindings_for_owned_module(
    path: &Path,
    module_path: &[String],
    settings: &NamespaceSettings,
) -> BTreeSet<String> {
    if !module_path_starts_with_owned_crate(module_path, settings) {
        return public_bindings_for_module(path, module_path);
    }

    let Some((src_root, relative_module_path)) =
        source_root_and_relative_module_path(path, module_path, settings)
    else {
        return BTreeSet::new();
    };

    for candidate in parent_module_files(&src_root, &relative_module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        return collect_bindings(&parsed.items, true);
    }

    BTreeSet::new()
}

fn module_path_starts_with_owned_crate(
    module_path: &[String],
    settings: &NamespaceSettings,
) -> bool {
    module_path
        .first()
        .is_some_and(|segment| settings.owned_crate_source_roots.contains_key(segment))
}

fn source_root_and_relative_module_path(
    path: &Path,
    module_path: &[String],
    settings: &NamespaceSettings,
) -> Option<(std::path::PathBuf, Vec<String>)> {
    if let Some(crate_name) = module_path.first()
        && let Some(src_root) = settings.owned_crate_source_roots.get(crate_name)
    {
        return Some((src_root.clone(), module_path[1..].to_vec()));
    }

    Some((source_root(path)?, module_path.to_vec()))
}

fn owned_facet_error_surface_target(
    items: &[Item],
    module_path: &[String],
    flat_error_name: &str,
) -> Option<Vec<String>> {
    if module_path.len() <= 1 {
        return None;
    }

    let companion_leaf = flat_error_name.strip_suffix("Error")?;
    if companion_leaf.is_empty()
        || namespace_scope_imports_binding_name(items, flat_error_name)
        || !namespace_scope_has_public_error_enum(items)
    {
        return None;
    }

    let local_leaf_candidates = namespace_local_public_companion_leaf_candidates(items);
    if !local_leaf_candidates.contains(companion_leaf) {
        return None;
    }
    if !namespace_public_error_variants_reference_flat_error(items, flat_error_name) {
        return None;
    }

    let mut preferred_path = module_path.to_vec();
    preferred_path.push("Error".to_string());
    Some(preferred_path)
}

fn candidate_child_facet_module_target(
    items: &[Item],
    module_path: &[String],
    settings: &NamespaceSettings,
) -> Option<(String, String)> {
    let parent_normalized = normalize_segment(module_path.last()?);
    if settings.weak_modules.contains(&parent_normalized)
        || settings.catch_all_modules.contains(&parent_normalized)
        || settings.organizational_modules.contains(&parent_normalized)
        || namespace_scope_has_public_error_enum(items)
    {
        return None;
    }

    let mut leaves = namespace_validated_leaf_wrapper_candidates(items);
    if leaves.len() != 1 {
        return None;
    }
    let (leaf_name, child_module_name) = leaves.pop()?;
    if normalize_segment(&child_module_name) == parent_normalized
        || namespace_scope_has_child_module_named(items, &child_module_name)
    {
        return None;
    }

    let broader_items = namespace_public_items_using_leaf(items, &leaf_name);
    if broader_items.is_empty() {
        return None;
    }

    Some((child_module_name, leaf_name))
}

fn candidate_root_facet_module_target(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    flat_error_name: &str,
    settings: &NamespaceSettings,
) -> Option<String> {
    let boundary_items = namespace_root_boundary_items(path, items, module_path, settings)?;
    let members = namespace_root_facet_members(&boundary_items);
    if members.len() < 2 || namespace_candidate_facet_members_look_like_scalar_bundle(&members) {
        return None;
    }

    let required_bindings = members
        .iter()
        .map(|(leaf_name, _)| leaf_name.clone())
        .chain(std::iter::once("Error".to_string()))
        .collect::<BTreeSet<_>>();
    let public_bindings = public_bindings_for_module(path, module_path);
    if !required_bindings.is_subset(&public_bindings) {
        return None;
    }

    members
        .into_iter()
        .find_map(|(leaf_name, member_error_name)| {
            (member_error_name == flat_error_name)
                .then(|| render_segments(&split_segments(&leaf_name), super::NameStyle::Snake))
        })
}

fn namespace_root_boundary_items(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    settings: &NamespaceSettings,
) -> Option<Vec<Item>> {
    if !namespace_root_facet_members(items).is_empty() {
        return Some(items.to_vec());
    }

    let boundary_module_name = namespace_reexported_error_child_module(items)?;
    let mut boundary_module_path = module_path.to_vec();
    boundary_module_path.push(boundary_module_name);
    module_items_for_module(path, &boundary_module_path, settings)
}

fn namespace_reexported_error_child_module(items: &[Item]) -> Option<String> {
    let mut matches = BTreeSet::new();

    for item in items {
        let Item::Use(item_use) = item else {
            continue;
        };
        if matches!(item_use.vis, Visibility::Inherited)
            || namespace_attrs_have_cfg_like(&item_use.attrs)
        {
            continue;
        }

        let mut leaves = Vec::new();
        flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
        for leaf in leaves {
            let binding = match leaf {
                UseLeaf::Direct(binding) | UseLeaf::Rename(binding) => binding,
                UseLeaf::Glob(_) => continue,
            };
            if binding.binding_name != "Error"
                || binding.source_name != "Error"
                || binding.full_path.len() < 2
            {
                continue;
            }
            let Some(parent_module) = binding.full_path.iter().rev().nth(1) else {
                continue;
            };
            matches.insert(parent_module.clone());
        }
    }

    (matches.len() == 1).then(|| matches.into_iter().next().expect("one boundary child"))
}

fn namespace_root_facet_members(items: &[Item]) -> Vec<(String, String)> {
    let mut members = Vec::new();

    for item in items {
        let Item::Enum(item_enum) = item else {
            continue;
        };
        if item_enum.ident != "Error"
            || !is_public(&item_enum.vis)
            || namespace_attrs_have_cfg_like(&item_enum.attrs)
        {
            continue;
        }

        for variant in &item_enum.variants {
            if namespace_attrs_have_cfg_like(&variant.attrs) {
                continue;
            }
            let leaf_name = variant.ident.to_string();
            let Some(flat_error_name) = namespace_variant_source_error_name(variant) else {
                continue;
            };
            let Some(companion_leaf) = flat_error_name.strip_suffix("Error") else {
                continue;
            };
            if !companion_leaf.is_empty() && companion_leaf == leaf_name {
                members.push((leaf_name, flat_error_name));
            }
        }
    }

    members.sort();
    members.dedup_by(|left, right| left.0 == right.0);
    members
}

fn namespace_candidate_facet_members_look_like_scalar_bundle(members: &[(String, String)]) -> bool {
    if members.len() < 2 {
        return false;
    }

    let scalar_tokens = [
        "code", "count", "id", "key", "label", "last4", "name", "number", "text", "title", "value",
    ];

    members.iter().all(|(leaf_name, _)| {
        split_segments(leaf_name)
            .into_iter()
            .map(|segment| segment.to_ascii_lowercase())
            .any(|segment| scalar_tokens.contains(&segment.as_str()))
    })
}

fn namespace_scope_has_public_error_enum(items: &[Item]) -> bool {
    items.iter().any(|item| {
        matches!(
            item,
            Item::Enum(ItemEnum {
                ident,
                vis,
                attrs,
                ..
            }) if ident == "Error" && is_public(vis) && !namespace_attrs_have_cfg_like(attrs)
        )
    })
}

fn namespace_scope_has_child_module_named(items: &[Item], module_name: &str) -> bool {
    items.iter().any(|item| {
        matches!(
            item,
            Item::Mod(ItemMod { ident, attrs, .. })
                if !namespace_attrs_have_cfg_like(attrs)
                    && normalize_segment(&ident.to_string()) == normalize_segment(module_name)
        )
    })
}

fn namespace_local_public_companion_leaf_candidates(items: &[Item]) -> BTreeSet<String> {
    let mut leaves = BTreeSet::new();

    for item in items {
        let Item::Struct(item_struct) = item else {
            continue;
        };
        if !is_public(&item_struct.vis)
            || namespace_attrs_have_cfg_like(&item_struct.attrs)
            || item_struct.ident == "Error"
            || !namespace_item_is_single_field_tuple_struct(item)
        {
            continue;
        }
        leaves.insert(item_struct.ident.to_string());
    }

    leaves
}

fn namespace_validated_leaf_wrapper_candidates(items: &[Item]) -> Vec<(String, String)> {
    let mut leaves = Vec::new();

    for item in items {
        let Item::Struct(item_struct) = item else {
            continue;
        };
        if !is_public(&item_struct.vis)
            || namespace_attrs_have_cfg_like(&item_struct.attrs)
            || !namespace_item_is_single_field_tuple_struct(item)
            || !namespace_attrs_have_nutype_validate(&item_struct.attrs)
        {
            continue;
        }

        let leaf_name = item_struct.ident.to_string();
        let child_module_name =
            render_segments(&split_segments(&leaf_name), super::NameStyle::Snake);
        leaves.push((leaf_name, child_module_name));
    }

    leaves
}

fn namespace_public_items_using_leaf(items: &[Item], leaf_name: &str) -> Vec<String> {
    let mut broader_items = Vec::new();

    for item in items {
        let Some((binding_name, is_public)) = public_item_binding(item) else {
            continue;
        };
        if !is_public
            || binding_name == leaf_name
            || binding_name == "Error"
            || !namespace_item_is_child_facet_owner_candidate(item)
            || namespace_attrs_have_cfg_like(namespace_item_attrs(item))
        {
            continue;
        }
        if namespace_public_item_mentions_leaf(item, leaf_name) {
            broader_items.push(binding_name);
        }
    }

    broader_items.sort();
    broader_items.dedup();
    broader_items
}

fn namespace_scope_imports_binding_name(items: &[Item], binding_name: &str) -> bool {
    items.iter().any(|item| {
        let Item::Use(item_use) = item else {
            return false;
        };
        let mut leaves = Vec::new();
        flatten_use_tree(Vec::new(), &item_use.tree, &mut leaves);
        leaves.into_iter().any(|leaf| match leaf {
            UseLeaf::Direct(binding) | UseLeaf::Rename(binding) => {
                binding.binding_name == binding_name
            }
            UseLeaf::Glob(_) => false,
        })
    })
}

fn namespace_public_error_variants_reference_flat_error(
    items: &[Item],
    flat_error_name: &str,
) -> bool {
    items.iter().any(|item| {
        let Item::Enum(item_enum) = item else {
            return false;
        };
        if item_enum.ident != "Error"
            || !is_public(&item_enum.vis)
            || namespace_attrs_have_cfg_like(&item_enum.attrs)
        {
            return false;
        }

        item_enum.variants.iter().any(|variant| {
            !namespace_attrs_have_cfg_like(&variant.attrs)
                && namespace_variant_source_error_name(variant)
                    .is_some_and(|candidate| candidate == flat_error_name)
        })
    })
}

fn namespace_variant_source_error_name(variant: &syn::Variant) -> Option<String> {
    let type_path = match &variant.fields {
        syn::Fields::Named(fields) => fields
            .named
            .iter()
            .find(|field| field.ident.as_ref().is_some_and(|ident| ident == "source"))
            .and_then(namespace_type_path_from_field),
        syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            namespace_type_path_from_field(fields.unnamed.first()?)
        }
        _ => None,
    }?;
    type_path
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn namespace_type_path_from_field(field: &syn::Field) -> Option<&syn::TypePath> {
    let syn::Type::Path(type_path) = &field.ty else {
        return None;
    };
    type_path.qself.is_none().then_some(type_path)
}

fn namespace_boundary_owner_module_path(module_path: &[String]) -> Vec<String> {
    match module_path.split_last() {
        Some((last, rest)) if matches!(normalize_segment(last).as_str(), "error" | "errors") => {
            rest.to_vec()
        }
        _ => module_path.to_vec(),
    }
}

fn boundary_owner_module_path(module_path: &[String]) -> Vec<String> {
    namespace_boundary_owner_module_path(module_path)
}

fn namespace_item_is_single_field_tuple_struct(item: &Item) -> bool {
    matches!(
        item,
        Item::Struct(ItemStruct {
            fields: syn::Fields::Unnamed(fields),
            ..
        }) if fields.unnamed.len() == 1
    )
}

fn namespace_attrs_have_nutype_validate(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("nutype"))
        .any(|attr| match &attr.meta {
            syn::Meta::List(list) => list.tokens.clone().into_iter().any(|token| {
                matches!(
                    token,
                    proc_macro2::TokenTree::Ident(ident) if ident == "validate"
                )
            }),
            _ => false,
        })
}

fn namespace_attrs_have_cfg_like(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
}

fn namespace_item_is_child_facet_owner_candidate(item: &Item) -> bool {
    matches!(
        item,
        Item::Struct(_)
            | Item::Enum(_)
            | Item::Trait(_)
            | Item::TraitAlias(_)
            | Item::Type(_)
            | Item::Union(_)
    )
}

fn namespace_public_item_mentions_leaf(item: &Item, leaf_name: &str) -> bool {
    struct LeafMentionVisitor<'a> {
        leaf_name: &'a str,
        found: bool,
    }

    impl<'ast> Visit<'ast> for LeafMentionVisitor<'_> {
        fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
            if let Some(ident) = type_path.path.segments.last()
                && ident.ident == self.leaf_name
            {
                self.found = true;
                return;
            }
            visit::visit_type_path(self, type_path);
        }
    }

    let mut visitor = LeafMentionVisitor {
        leaf_name,
        found: false,
    };
    visitor.visit_item(item);
    visitor.found
}

fn namespace_family_shared_prefix(
    binding_name: &str,
    settings: &NamespaceSettings,
) -> Option<Vec<String>> {
    let binding_segments = split_segments(binding_name);
    if binding_segments.len() < 2 {
        return None;
    }

    let tail = binding_segments.last()?;
    if !namespace_visible_family_tail(tail, settings) {
        return None;
    }

    Some(
        binding_segments[..binding_segments.len() - 1]
            .iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect(),
    )
}

fn namespace_visible_family_tail(segment: &str, settings: &NamespaceSettings) -> bool {
    matches_generic_noun(segment, settings)
        || (!is_unreadable_short_leaf(segment)
            && !namespace_leaf_context_is_not_actionable("", segment))
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

    if leaf_normalized.ends_with(&module_segments) && {
        let shorter_segments = &leaf_segments[..leaf_segments.len() - module_segments.len()];
        !shorter_segments.is_empty()
            && suffix_overlap_is_actionable(
                parent_module,
                full_path,
                &render_segments(shorter_segments, style),
            )
    } {
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
            let shorter_leaf = render_segments(shorter_segments, style);
            if namespace_leaf_context_is_not_actionable(parent_module, &shorter_leaf) {
                return None;
            }
            return Some(shorter_leaf);
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
    let Some(parent_module) = full_path.iter().rev().nth(1) else {
        return false;
    };
    if namespace_leaf_context_is_not_actionable(parent_module, shorter_leaf) {
        return false;
    }
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

fn suffix_overlap_is_actionable(
    parent_module: &str,
    full_path: &[String],
    shorter_leaf: &str,
) -> bool {
    if full_path.len() > 3 {
        return false;
    }
    if namespace_leaf_context_is_not_actionable(parent_module, shorter_leaf) {
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

fn namespace_leaf_context_is_not_actionable(_parent_module: &str, shorter_leaf: &str) -> bool {
    let shorter_tokens = split_segments(shorter_leaf)
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if shorter_tokens.is_empty() {
        return true;
    }

    let weak_tokens = [
        "box",
        "into",
        "iter",
        "layer",
        "no",
        "optional",
        "parts",
        "set",
        "setup",
        "statement",
    ];
    if shorter_tokens
        .iter()
        .all(|token| weak_tokens.contains(&token.as_str()))
    {
        return true;
    }

    false
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

fn namespace_family_unsupported_construct_message(
    action: UnsupportedNamespaceFamilyAction,
    source_names: &[String],
    constructs: &BTreeSet<String>,
) -> (&'static str, String) {
    let action = match action {
        UnsupportedNamespaceFamilyAction::Import => "import",
        UnsupportedNamespaceFamilyAction::ReExport => "re-export",
        UnsupportedNamespaceFamilyAction::Alias => "type alias",
    };
    let rendered_names = source_names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let message = if source_names
        .iter()
        .all(|name| name != "Error" && name.ends_with("Error"))
    {
        let family_label = if source_names.len() > 1 {
            "flat leaf failure family"
        } else {
            "flat leaf failure"
        };
        format!(
            "skipped namespace-family inference for {rendered_names} in this {action} because source-level analysis saw {}; verify manually whether this {family_label} belongs under an owning facet before changing the path",
            render_unsupported_constructs(constructs),
        )
    } else {
        format!(
            "skipped namespace-family inference for {rendered_names} in this {action} because source-level analysis saw {}; verify the owning family manually before changing the visible path",
            render_unsupported_constructs(constructs),
        )
    };
    ("namespace_family_unsupported_construct", message)
}

struct GroupedUnsupportedUse {
    source_names: Vec<String>,
    constructs: BTreeSet<String>,
}

fn group_pending_unsupported_uses(pending: &[PendingUnsupportedUse]) -> Vec<GroupedUnsupportedUse> {
    let mut grouped = Vec::<GroupedUnsupportedUse>::new();

    for entry in pending {
        if let Some(existing) = grouped
            .iter_mut()
            .find(|existing| existing.constructs == entry.constructs)
        {
            existing.source_names.push(entry.source_name.clone());
            continue;
        }

        grouped.push(GroupedUnsupportedUse {
            source_names: vec![entry.source_name.clone()],
            constructs: entry.constructs.clone(),
        });
    }

    for group in &mut grouped {
        group.source_names.sort();
        group.source_names.dedup();
    }

    grouped
}

fn render_unsupported_constructs(constructs: &BTreeSet<String>) -> String {
    constructs
        .iter()
        .map(|construct| format!("`{construct}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn public_bindings_for_module(path: &Path, module_path: &[String]) -> BTreeSet<String> {
    bindings_for_module(path, module_path, true)
}

fn module_bindings_for_module(path: &Path, module_path: &[String]) -> BTreeSet<String> {
    bindings_for_module(path, module_path, false)
}

fn namespace_family_binding_info_for_module(
    path: &Path,
    module_path: &[String],
) -> NamespaceFamilyBindingInfo {
    if let Some(info) = inline_namespace_family_binding_info_for_module(path, module_path) {
        return info;
    }

    let Some(src_root) = source_root(path) else {
        return NamespaceFamilyBindingInfo {
            bindings: BTreeSet::new(),
            unsupported_constructs: BTreeSet::new(),
        };
    };

    for candidate in parent_module_files(&src_root, module_path) {
        let Ok(src) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&src) else {
            continue;
        };
        return collect_namespace_family_binding_info(&parsed.items);
    }

    NamespaceFamilyBindingInfo {
        bindings: BTreeSet::new(),
        unsupported_constructs: BTreeSet::new(),
    }
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

fn inline_namespace_family_binding_info_for_module(
    path: &Path,
    module_path: &[String],
) -> Option<NamespaceFamilyBindingInfo> {
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
    Some(collect_namespace_family_binding_info(items))
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

fn module_path_exists_in_current_crate(path: &Path, module_path: &[String]) -> bool {
    if module_path.is_empty() {
        return true;
    }

    if inline_module_items_for_module(path, module_path).is_some() {
        return true;
    }

    let Some(src_root) = source_root(path) else {
        return false;
    };

    parent_module_files(&src_root, module_path)
        .into_iter()
        .any(|candidate| candidate.is_file())
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

fn collect_namespace_family_binding_info(items: &[Item]) -> NamespaceFamilyBindingInfo {
    let mut bindings = BTreeSet::new();
    let mut unsupported_constructs = BTreeSet::new();

    for item in items {
        unsupported_constructs.extend(namespace_family_constructs_for_item(item));

        match item {
            Item::Use(item_use) if !matches!(item_use.vis, Visibility::Inherited) => {
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
                if let Some((binding_name, _)) = public_item_binding(item) {
                    bindings.insert(binding_name);
                }
            }
        }
    }

    NamespaceFamilyBindingInfo {
        bindings,
        unsupported_constructs,
    }
}

fn namespace_family_constructs_for_item(item: &Item) -> BTreeSet<String> {
    if !item_participates_in_namespace_family(item) {
        return BTreeSet::new();
    }

    let mut constructs = BTreeSet::new();

    for attr in namespace_item_attrs(item) {
        if attr.path().is_ident("cfg") {
            constructs.insert("#[cfg]".to_string());
        }
        if attr.path().is_ident("cfg_attr") {
            constructs.insert("#[cfg_attr]".to_string());
        }
    }

    if let Item::Macro(item_macro) = item {
        constructs.insert(namespace_item_macro_observation_construct(item_macro).to_string());
    }

    constructs
}

fn item_participates_in_namespace_family(item: &Item) -> bool {
    matches!(item, Item::Macro(_))
        || public_item_binding(item).is_some()
        || matches!(item, Item::Use(item_use) if !matches!(item_use.vis, Visibility::Inherited))
}

fn namespace_item_attrs(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(item_const) => &item_const.attrs,
        Item::Enum(item_enum) => &item_enum.attrs,
        Item::ExternCrate(item_extern_crate) => &item_extern_crate.attrs,
        Item::Fn(item_fn) => &item_fn.attrs,
        Item::ForeignMod(item_foreign_mod) => &item_foreign_mod.attrs,
        Item::Impl(item_impl) => &item_impl.attrs,
        Item::Macro(item_macro) => &item_macro.attrs,
        Item::Mod(item_mod) => &item_mod.attrs,
        Item::Static(item_static) => &item_static.attrs,
        Item::Struct(item_struct) => &item_struct.attrs,
        Item::Trait(item_trait) => &item_trait.attrs,
        Item::TraitAlias(item_trait_alias) => &item_trait_alias.attrs,
        Item::Type(item_type) => &item_type.attrs,
        Item::Union(item_union) => &item_union.attrs,
        Item::Use(item_use) => &item_use.attrs,
        _ => &[],
    }
}

fn namespace_item_macro_observation_construct(item_macro: &syn::ItemMacro) -> &'static str {
    if item_macro.mac.path.is_ident("include") {
        "include!"
    } else if item_macro.mac.path.is_ident("macro_rules") {
        "macro_rules!"
    } else {
        "item macro"
    }
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
