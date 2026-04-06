use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use syn::{
    BinOp, Expr, ExprBinary, File, FnArg, GenericArgument, ImplItem, ImplItemFn, Item, ItemConst,
    ItemEnum, ItemFn, ItemImpl, ItemMacro, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemTraitAlias, ItemType, ItemUnion, ItemUse, Lit, LitStr, PathArguments, ReturnType, Stmt,
    Type, UseTree, spanned::Spanned, visit::Visit,
};

use super::{
    Diagnostic, NameStyle, NamespaceSettings, detect_name_style, inferred_file_module_path,
    is_public, normalize_segment, parent_module_files, render_segments, source_root,
    split_segments, unraw_ident,
};

pub(super) struct Analysis {
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

struct SharedHeadSemanticModuleSuggestion {
    shared_tail: Option<String>,
    surface: String,
}

struct SemanticModuleSurfaceCandidate {
    preferred_path: String,
    module_name: String,
    shorter_leaf: String,
}

#[derive(Clone)]
struct ChildModuleSurfaceExport {
    parent_binding: String,
    child_leaf: String,
}

struct StringFieldSignal {
    name: String,
    scaffold_like: bool,
    has_strong_metadata_token: bool,
}

#[derive(Default)]
struct TraitSurfaceCatalog {
    from_targets_by_source: BTreeMap<String, BTreeSet<String>>,
    display_impl_lines: BTreeMap<String, usize>,
    error_impl_lines: BTreeMap<String, usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BitMaskPatternKind {
    Check,
    Assembly,
}

struct FlagBitPatternUse {
    boundary_name: String,
    const_name: String,
    kind: BitMaskPatternKind,
    line: usize,
}

struct FlagBitPatternVisitor<'a> {
    flag_constants: &'a BTreeSet<String>,
    uses: Vec<FlagBitPatternUse>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MetadataHelperKind {
    StringSurface,
    TypedSurface,
}

#[derive(Clone, Copy)]
enum StrumCase {
    KebabCase,
    SnakeCase,
    ScreamingSnakeCase,
    Lowercase,
}

struct ScopeSurfaceContext<'a> {
    public_bindings: &'a BTreeSet<String>,
    suppressed_child_module_exports: &'a BTreeSet<String>,
}

#[derive(Clone, Copy)]
struct ScopeFlags {
    path_is_public: bool,
    allow_candidate_semantic_modules: bool,
}

#[derive(Clone)]
struct SemanticInferenceObservationGap {
    line: usize,
    constructs: BTreeSet<String>,
}

struct SemanticChildModuleBindings {
    bindings: BTreeMap<String, BTreeSet<String>>,
    observation_gap: Option<SemanticInferenceObservationGap>,
}

struct ChildModulePublicBindings {
    bindings: BTreeSet<String>,
    observation_gap_constructs: BTreeSet<String>,
}

#[derive(Clone)]
struct ScopeModuleBinding {
    line: usize,
    module_name: String,
}

#[derive(Clone)]
struct ScopeItemBinding {
    line: usize,
    binding_name: String,
}

struct SharedHeadSemanticFamilyChoice {
    shared_head_segments: Vec<String>,
    members: Vec<(usize, String)>,
    suggestion: SharedHeadSemanticModuleSuggestion,
}

pub(super) fn analyze_api_shape_rules(
    path: &Path,
    parsed: &File,
    settings: &NamespaceSettings,
) -> Analysis {
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
        diagnostics.push(Diagnostic::policy_error(
            Some(path.to_path_buf()),
            Some(1),
            "api_organizational_submodule_flatten",
            format!(
                "`{}` leaks organizational `{}` into the public API; prefer `{}` and keep `{}` private",
                render_public_path(&inferred_module_path, &flatten_leaf),
                inferred_module_path.last().expect("non-empty inferred module path"),
                render_preferred_public_path(
                    &inferred_module_path[..inferred_module_path.len() - 1],
                    &flatten_leaf,
                    settings,
                ),
                inferred_module_path.last().expect("non-empty inferred module path"),
            ),
        ));
    }

    analyze_scope(
        path,
        &parsed.items,
        &inferred_module_path,
        ScopeFlags {
            path_is_public: inferred_is_public,
            allow_candidate_semantic_modules: !module_path_contains_internal_namespace(
                &inferred_module_path,
            ),
        },
        settings,
        &mut diagnostics,
    );

    Analysis { diagnostics }
}

fn analyze_scope(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    scope_flags: ScopeFlags,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let public_bindings = collect_scope_public_bindings(items);
    let suppressed_child_module_exports = analyze_candidate_semantic_modules(
        path,
        items,
        module_path,
        scope_flags,
        &public_bindings,
        settings,
        diagnostics,
    );
    if !internal_candidate_semantic_module_inference_blocked(
        items,
        module_path,
        scope_flags,
        settings,
    ) {
        analyze_internal_candidate_module_families(
            path,
            items,
            module_path,
            scope_flags,
            settings,
            diagnostics,
        );
        analyze_internal_candidate_item_families(
            path,
            items,
            module_path,
            scope_flags,
            settings,
            diagnostics,
        );
    }
    let scope_context = ScopeSurfaceContext {
        public_bindings: &public_bindings,
        suppressed_child_module_exports: &suppressed_child_module_exports,
    };
    analyze_stringly_public_surfaces(path, items, scope_flags.path_is_public, diagnostics);
    analyze_modeling_api_surfaces(
        path,
        items,
        scope_flags.path_is_public,
        settings,
        diagnostics,
    );

    for item in items {
        if !matches!(item, Item::Mod(_)) {
            analyze_maybe_some_callsite_item(path, item, diagnostics);
        }
    }

    for item in items {
        match item {
            Item::Mod(item_mod) => analyze_module_item(
                path,
                item_mod,
                module_path,
                scope_flags,
                &scope_context,
                settings,
                diagnostics,
            ),
            Item::Use(item_use) => {
                if scope_flags.path_is_public {
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
            _ => analyze_item_shape(
                path,
                item,
                items,
                module_path,
                scope_flags.path_is_public,
                settings,
                diagnostics,
            ),
        }
    }
}

fn analyze_maybe_some_callsite_item(path: &Path, item: &Item, diagnostics: &mut Vec<Diagnostic>) {
    let mut visitor = MaybeSomeMethodCallVisitor { path, diagnostics };
    visitor.visit_item(item);
}

fn analyze_module_item(
    path: &Path,
    item_mod: &ItemMod,
    module_path: &[String],
    scope_flags: ScopeFlags,
    scope_context: &ScopeSurfaceContext<'_>,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let module_name = item_mod.ident.to_string();
    let normalized = normalize_segment(&module_name);
    let line = item_mod.span().start().line;
    let module_is_public = scope_flags.path_is_public && is_public(&item_mod.vis);

    if module_is_public {
        if settings.catch_all_modules.contains(&normalized) {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "api_catch_all_module",
                format!(
                    "`{module_name}` is a catch-all public module; prefer a stable domain or facet"
                ),
            ));
        }

        if settings.organizational_modules.contains(&normalized)
            && let Some(flatten_leaf) = organizational_flatten_candidate(
                item_mod.content.as_ref().map(|(_, nested)| nested),
                settings,
            )
        {
            diagnostics.push(Diagnostic::policy_error(
                Some(path.to_path_buf()),
                Some(line),
                "api_organizational_submodule_flatten",
                format!(
                    "`{}` leaks organizational `{module_name}` into the public API; prefer `{}` and keep `{module_name}` private",
                    render_public_path_with_module(module_path, &module_name, &flatten_leaf),
                    render_preferred_public_path(module_path, &flatten_leaf, settings)
                ),
            ));
        }

        if module_path
            .last()
            .is_some_and(|parent| normalize_segment(parent) == normalized)
        {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "api_repeated_module_segment",
                format!(
                    "nested module path repeats `{module_name}`; flatten or rename the redundant segment"
                ),
            ));
        }
    }

    if !module_is_public {
        if settings.catch_all_modules.contains(&normalized) {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "internal_catch_all_module",
                format!(
                    "internal module `{module_name}` is a catch-all bucket; prefer a stable domain or facet"
                ),
            ));
        }

        if settings.organizational_modules.contains(&normalized) && !module_path.is_empty() {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "internal_organizational_submodule_flatten",
                format!(
                    "internal organizational module `{module_name}` should usually be flattened or renamed so the category does not carry the naming burden"
                ),
            ));
        }

        if module_path
            .last()
            .is_some_and(|parent| normalize_segment(parent) == normalized)
        {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "internal_repeated_module_segment",
                format!(
                    "internal nested module path repeats `{module_name}`; flatten or rename the redundant segment"
                ),
            ));
        }

        if let Some((head_module, tail_module)) =
            namespace_preserving_module_tail_candidate(&module_name, settings)
        {
            let path_text = render_public_path(module_path, &module_name);
            let mut preferred_module_path = module_path.to_vec();
            preferred_module_path.push(head_module);
            let preferred_path = render_public_path(&preferred_module_path, &tail_module);

            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "internal_flat_namespace_preserving_module",
                format!(
                    "internal module `{path_text}` flattens namespace-preserving `{tail_module}` facet; prefer `{preferred_path}`",
                ),
            ));
        }
    }

    if is_surface_export_candidate(module_path, scope_flags.path_is_public, settings)
        && is_public(&item_mod.vis)
        && !scope_context
            .suppressed_child_module_exports
            .contains(&normalized)
        && let Some(surface_export) = child_module_surface_export_candidate(
            path,
            module_path,
            Some(item_mod),
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
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_missing_parent_surface_export",
            format!(
                "parent surface is missing `{}`; re-export it so callers do not have to use `{}`",
                render_public_path(module_path, &surface_export.parent_binding),
                render_public_path_with_module(
                    module_path,
                    &module_name,
                    &surface_export.child_leaf,
                ),
            ),
        ));
    }

    if let Some((_, nested)) = &item_mod.content {
        let mut next_path = module_path.to_vec();
        next_path.push(module_name);
        analyze_scope(
            path,
            nested,
            &next_path,
            ScopeFlags {
                path_is_public: module_is_public,
                allow_candidate_semantic_modules: scope_flags.allow_candidate_semantic_modules
                    && !module_is_hidden_or_internal(item_mod),
            },
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
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "api_missing_parent_surface_export",
                format!(
                    "parent surface is missing `{}`; re-export it so callers do not have to use `{}`",
                    render_public_path(module_path, &surface_export.parent_binding),
                    render_public_path_with_module(
                        module_path,
                        &leaf.binding_name,
                        &surface_export.child_leaf,
                    ),
                ),
            ));
        }

        analyze_public_leaf(
            path,
            line,
            scope_items,
            module_path,
            &leaf.binding_name,
            Some(&leaf),
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

fn collect_scope_public_module_bindings(
    items: &[Item],
    path_is_public: bool,
    settings: &NamespaceSettings,
) -> Vec<ScopeModuleBinding> {
    collect_scope_module_bindings(items, path_is_public, true, settings)
}

fn collect_scope_internal_module_bindings(
    items: &[Item],
    path_is_public: bool,
    settings: &NamespaceSettings,
) -> Vec<ScopeModuleBinding> {
    collect_scope_module_bindings(items, path_is_public, false, settings)
}

fn collect_scope_module_bindings(
    items: &[Item],
    path_is_public: bool,
    public_only: bool,
    settings: &NamespaceSettings,
) -> Vec<ScopeModuleBinding> {
    items
        .iter()
        .filter_map(|item| {
            let Item::Mod(item_mod) = item else {
                return None;
            };
            if module_is_hidden_or_internal(item_mod) {
                return None;
            }
            let module_is_public = path_is_public && is_public(&item_mod.vis);
            if public_only != module_is_public {
                return None;
            }

            let module_name = item_mod.ident.to_string();
            let normalized = normalize_segment(&module_name);
            if settings.weak_modules.contains(&normalized)
                || settings.catch_all_modules.contains(&normalized)
                || settings.organizational_modules.contains(&normalized)
            {
                return None;
            }

            Some(ScopeModuleBinding {
                line: item_mod.span().start().line,
                module_name,
            })
        })
        .collect()
}

fn collect_scope_internal_item_bindings(
    items: &[Item],
    path_is_public: bool,
) -> Vec<ScopeItemBinding> {
    items
        .iter()
        .filter_map(|item| {
            if !is_internal_shape_candidate_item(item) {
                return None;
            }

            let (line, binding_name, is_item_public) = public_item_leaf(item)?;
            if path_is_public && is_item_public {
                return None;
            }

            Some(ScopeItemBinding { line, binding_name })
        })
        .collect()
}

fn collect_scope_public_enum_names(items: &[Item]) -> BTreeSet<String> {
    let mut enums = BTreeSet::new();

    for item in items {
        if let Item::Enum(item_enum) = item
            && is_public(&item_enum.vis)
        {
            enums.insert(unraw_ident(&item_enum.ident));
        }
    }

    enums
}

fn collect_scope_public_error_type_names(items: &[Item]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    for item in items {
        let Some((_, binding_name, is_public)) = public_item_leaf(item) else {
            continue;
        };
        if is_public && looks_like_error_type_name(&binding_name) {
            names.insert(binding_name);
        }
    }

    names
}

fn collect_scope_flag_constant_names(items: &[Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Const(item_const)
                if flag_const_name(&item_const.ident.to_string()).is_some()
                    && type_is_bitflag_integer(&item_const.ty) =>
            {
                Some(item_const.ident.to_string())
            }
            _ => None,
        })
        .collect()
}

fn collect_impl_flag_constant_names(item_impl: &ItemImpl) -> BTreeSet<String> {
    item_impl
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Const(item_const)
                if flag_const_name(&item_const.ident.to_string()).is_some()
                    && type_is_bitflag_integer(&item_const.ty) =>
            {
                Some(item_const.ident.to_string())
            }
            _ => None,
        })
        .collect()
}

fn collect_scope_from_str_targets(items: &[Item]) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();

    for item in items {
        let Item::Impl(item_impl) = item else {
            continue;
        };
        let Some((_, trait_path, _)) = &item_impl.trait_ else {
            continue;
        };
        let Some(trait_name) = trait_path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            continue;
        };
        if trait_name != "FromStr" {
            continue;
        }
        if let Some(self_ty) = type_leaf_ident(&item_impl.self_ty) {
            targets.insert(self_ty);
        }
    }

    targets
}

fn analyze_stringly_public_surfaces(
    path: &Path,
    items: &[Item],
    path_is_public: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !path_is_public {
        return;
    }

    let public_enum_names = collect_scope_public_enum_names(items);
    let from_str_targets = if public_enum_names.is_empty() {
        BTreeSet::new()
    } else {
        collect_scope_from_str_targets(items)
    };

    for item in items {
        match item {
            Item::Impl(item_impl) if !public_enum_names.is_empty() => {
                analyze_public_enum_impl_string_helpers(
                    path,
                    item_impl,
                    &public_enum_names,
                    &from_str_targets,
                    diagnostics,
                )
            }
            Item::Fn(item_fn) => {
                if !public_enum_names.is_empty() {
                    analyze_public_enum_string_helper_functions(
                        path,
                        item_fn,
                        &public_enum_names,
                        diagnostics,
                    );
                    analyze_public_parse_helpers(
                        path,
                        item_fn,
                        &public_enum_names,
                        &from_str_targets,
                        diagnostics,
                    );
                }
            }
            Item::Const(item_const) => {
                analyze_public_stringly_protocol_collection_const(path, item_const, diagnostics);
            }
            Item::Static(item_static) => {
                analyze_public_stringly_protocol_collection_static(path, item_static, diagnostics);
            }
            Item::Struct(item_struct) => {
                analyze_public_stringly_model_scaffolds(path, item_struct, diagnostics);
            }
            _ => {}
        }
    }
}

fn analyze_modeling_api_surfaces(
    path: &Path,
    items: &[Item],
    path_is_public: bool,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !path_is_public {
        return;
    }

    let public_enum_names = collect_scope_public_enum_names(items);
    let public_error_type_names = collect_scope_public_error_type_names(items);
    let scope_flag_constants = collect_scope_flag_constant_names(items);
    let trait_surfaces = collect_scope_trait_surfaces(items);

    analyze_standalone_builder_surfaces(path, items, diagnostics);
    analyze_repeated_parameter_clusters(path, items, diagnostics);
    analyze_public_manual_flag_sets(path, items, diagnostics);
    if !public_error_type_names.is_empty() {
        analyze_public_manual_error_surfaces(
            path,
            &public_error_type_names,
            &trait_surfaces,
            diagnostics,
        );
    }

    for item in items {
        match item {
            Item::Enum(item_enum) => {
                analyze_public_enum_strum_serialize_all_candidate(path, item_enum, diagnostics);
            }
            Item::Struct(item_struct) => {
                analyze_public_struct_boolean_protocol_decisions(path, item_struct, diagnostics);
                analyze_public_struct_boolean_flag_clusters(path, item_struct, diagnostics);
                analyze_public_struct_flag_bit_fields(path, item_struct, diagnostics);
                analyze_public_struct_string_error_fields(path, item_struct, diagnostics);
                analyze_public_struct_anyhow_error_fields(path, item_struct, diagnostics);
                analyze_public_struct_semantic_scalar_fields(
                    path,
                    item_struct,
                    settings,
                    diagnostics,
                );
                analyze_public_struct_numeric_scalar_fields(
                    path,
                    item_struct,
                    settings,
                    diagnostics,
                );
                analyze_public_struct_key_value_bag_fields(
                    path,
                    item_struct,
                    settings,
                    diagnostics,
                );
                analyze_public_struct_protocol_integer_fields(path, item_struct, diagnostics);
                analyze_public_struct_raw_id_fields(path, item_struct, diagnostics);
            }
            Item::Fn(item_fn) => {
                analyze_public_fn_boolean_protocol_decisions(path, item_fn, diagnostics);
                analyze_public_fn_boolean_flag_clusters(path, item_fn, diagnostics);
                analyze_public_fn_flag_bit_boundaries(
                    path,
                    item_fn,
                    &scope_flag_constants,
                    diagnostics,
                );
                analyze_public_fn_builder_candidate(path, item_fn, diagnostics);
                analyze_public_stringly_protocol_parameters(path, item_fn, diagnostics);
                analyze_public_fn_typed_error_surfaces(path, item_fn, diagnostics);
                analyze_public_fn_semantic_scalar_boundaries(path, item_fn, settings, diagnostics);
                analyze_public_fn_key_value_bag_surfaces(path, item_fn, settings, diagnostics);
                analyze_public_fn_protocol_integer_boundaries(path, item_fn, diagnostics);
                analyze_public_fn_raw_id_boundaries(path, item_fn, diagnostics);
            }
            Item::Type(item_type) => {
                analyze_public_type_alias_error_surfaces(path, item_type, diagnostics);
                analyze_public_type_alias_raw_id_surface(path, item_type, diagnostics);
                analyze_public_type_alias_key_value_bag_surface(
                    path,
                    item_type,
                    settings,
                    diagnostics,
                );
            }
            Item::Impl(item_impl) => {
                analyze_public_impl_boolean_protocol_decisions(path, item_impl, diagnostics);
                analyze_public_impl_boolean_flag_clusters(path, item_impl, diagnostics);
                analyze_public_impl_flag_bit_boundaries(
                    path,
                    item_impl,
                    &scope_flag_constants,
                    diagnostics,
                );
                analyze_public_impl_builder_candidates(path, item_impl, diagnostics);
                analyze_public_impl_stringly_protocol_parameters(path, item_impl, diagnostics);
                analyze_public_impl_typed_error_surfaces(path, item_impl, diagnostics);
                analyze_public_impl_semantic_scalar_boundaries(
                    path,
                    item_impl,
                    settings,
                    diagnostics,
                );
                analyze_public_impl_key_value_bag_surfaces(path, item_impl, settings, diagnostics);
                analyze_public_impl_protocol_integer_boundaries(path, item_impl, diagnostics);
                analyze_public_impl_raw_id_boundaries(path, item_impl, diagnostics);
                analyze_forwarding_compat_wrappers(path, item_impl, &trait_surfaces, diagnostics);

                if !public_enum_names.is_empty() {
                    analyze_public_enum_manual_display_surfaces(
                        path,
                        item_impl,
                        &public_enum_names,
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
}

fn analyze_repeated_parameter_clusters(
    path: &Path,
    items: &[Item],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut families = BTreeMap::<Vec<(String, String)>, Vec<(usize, String)>>::new();

    for item in items {
        match item {
            Item::Fn(item_fn) => {
                if !is_public(&item_fn.vis) || has_builder_attribute(&item_fn.attrs) {
                    continue;
                }
                let item_name = item_fn.sig.ident.to_string();
                let typed_inputs = typed_inputs(&item_fn.sig.inputs);
                let Some(cluster) = repeated_parameter_cluster_signature(
                    &item_name,
                    &typed_inputs,
                    &item_fn.sig.output,
                ) else {
                    continue;
                };
                families
                    .entry(cluster)
                    .or_default()
                    .push((item_fn.span().start().line, item_name));
            }
            Item::Impl(item_impl) => {
                if item_impl.trait_.is_some() {
                    continue;
                }
                let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
                    continue;
                };
                for item in &item_impl.items {
                    let ImplItem::Fn(method) = item else {
                        continue;
                    };
                    if !is_public(&method.vis) || has_builder_attribute(&method.attrs) {
                        continue;
                    }

                    let method_name = method.sig.ident.to_string();
                    let typed_inputs = typed_inputs(&method.sig.inputs);
                    let Some(cluster) = repeated_parameter_cluster_signature(
                        &method_name,
                        &typed_inputs,
                        &method.sig.output,
                    ) else {
                        continue;
                    };
                    families.entry(cluster).or_default().push((
                        method.span().start().line,
                        format!("{self_type}::{method_name}"),
                    ));
                }
            }
            _ => {}
        }
    }

    for (cluster, members) in families {
        if members.len() < 2 {
            continue;
        }

        let line = members
            .iter()
            .map(|(line, _)| *line)
            .min()
            .expect("cluster has members");
        let item_names = members
            .iter()
            .map(|(_, name)| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let param_names = cluster
            .iter()
            .map(|(name, _)| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_repeated_parameter_cluster",
            format!(
                "public entrypoints {item_names} repeat the same positional parameter cluster ({param_names}); prefer a shared options type or `bon` builder instead of duplicating the call shape"
            ),
        ));
    }
}

fn collect_scope_trait_surfaces(items: &[Item]) -> TraitSurfaceCatalog {
    let mut catalog = TraitSurfaceCatalog::default();

    for item in items {
        let Item::Impl(item_impl) = item else {
            continue;
        };
        let Some((_, trait_path, _)) = &item_impl.trait_ else {
            continue;
        };
        let Some(trait_name) = trait_path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            continue;
        };

        let Some(target) = type_leaf_ident(&item_impl.self_ty) else {
            continue;
        };

        if trait_name.as_str() == "From" {
            let Some(source) = trait_first_generic_type_leaf_ident(trait_path) else {
                continue;
            };
            catalog
                .from_targets_by_source
                .entry(source)
                .or_default()
                .insert(target.clone());
        }

        if trait_path_is_display(trait_path) {
            catalog
                .display_impl_lines
                .entry(target.clone())
                .or_insert(item_impl.span().start().line);
        }

        if trait_path_is_error(trait_path) {
            catalog
                .error_impl_lines
                .entry(target)
                .or_insert(item_impl.span().start().line);
        }
    }

    catalog
}

fn trait_path_is_display(trait_path: &syn::Path) -> bool {
    trait_path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Display")
}

fn trait_path_is_error(trait_path: &syn::Path) -> bool {
    trait_path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Error")
}

fn analyze_public_manual_flag_sets(path: &Path, items: &[Item], diagnostics: &mut Vec<Diagnostic>) {
    let flag_consts = items
        .iter()
        .filter_map(|item| match item {
            Item::Const(item_const)
                if is_public(&item_const.vis)
                    && flag_const_name(&item_const.ident.to_string()).is_some()
                    && type_is_bitflag_integer(&item_const.ty) =>
            {
                Some((item_const.span().start().line, item_const.ident.to_string()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if flag_consts.len() < 3 {
        return;
    }

    let line = flag_consts
        .iter()
        .map(|(line, _)| *line)
        .min()
        .expect("flag consts present");
    let names = flag_consts
        .iter()
        .map(|(_, name)| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(line),
        "api_manual_flag_set",
        format!(
            "public integer flag constants {names} model a manual bit-set surface; prefer a typed flags boundary instead of raw integer bits"
        ),
    ));
}

fn analyze_public_manual_error_surfaces(
    path: &Path,
    public_error_type_names: &BTreeSet<String>,
    trait_surfaces: &TraitSurfaceCatalog,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for error_name in public_error_type_names {
        let Some(display_line) = trait_surfaces.display_impl_lines.get(error_name) else {
            continue;
        };
        let Some(error_line) = trait_surfaces.error_impl_lines.get(error_name) else {
            continue;
        };
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some((*display_line).min(*error_line)),
            "api_manual_error_surface",
            format!(
                "public error `{error_name}` manually implements both `Display` and `Error`; keep the caller-facing error surface focused and typed instead of pushing boundary failures through catch-all formatting"
            ),
        ));
    }
}

fn analyze_public_struct_boolean_flag_clusters(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let bool_fields = matching_public_named_fields(fields, |_| true, type_is_bool);
    if bool_fields.len() < 2
        || bool_fields
            .iter()
            .all(|name| looks_like_runtime_toggle_name(name))
    {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_boolean_flag_cluster",
        format!(
            "public struct `{}` carries several boolean flags ({}); prefer a typed options or mode surface when those flags jointly shape behavior",
            item_struct.ident,
            render_name_list(&bool_fields),
        ),
    ));
}

fn analyze_public_struct_flag_bit_fields(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let raw_fields =
        matching_public_named_fields(fields, looks_like_flag_bits_name, type_is_bitflag_integer);
    if raw_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_manual_flag_set",
        format!(
            "public struct `{}` exposes raw bit-flag field(s) {}; prefer a typed flags boundary instead of manual `u32` or `u64` masks",
            item_struct.ident,
            render_name_list(&raw_fields),
        ),
    ));
}

fn analyze_public_struct_string_error_fields(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let struct_name = item_struct.ident.to_string();
    let error_fields = matching_public_named_fields(
        fields,
        |name| looks_like_error_field_name(&struct_name, name),
        type_is_string_field,
    );
    if error_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_string_error_surface",
        format!(
            "public struct `{}` stores boundary error text as raw string field(s) {}; prefer a typed error surface with named variants or focused error data",
            item_struct.ident,
            render_name_list(&error_fields),
        ),
    ));
}

fn analyze_public_struct_anyhow_error_fields(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let anyhow_fields = matching_public_named_fields(fields, |_| true, type_is_anyhow_error_type);
    if anyhow_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_anyhow_error_surface",
        format!(
            "public struct `{}` exposes `anyhow::Error` field(s) {}; keep `anyhow` internal and expose a crate-owned typed error surface at the boundary",
            item_struct.ident,
            render_name_list(&anyhow_fields),
        ),
    ));
}

fn analyze_public_struct_semantic_scalar_fields(
    path: &Path,
    item_struct: &ItemStruct,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let raw_fields = matching_public_named_fields(
        fields,
        |name| looks_like_semantic_string_scalar_name(name, settings),
        type_is_string_field,
    );
    if raw_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_semantic_string_scalar",
        format!(
            "public struct `{}` carries semantic scalar field(s) {} as raw strings; prefer typed boundary values or focused newtypes",
            item_struct.ident,
            render_name_list(&raw_fields),
        ),
    ));
}

fn analyze_public_struct_numeric_scalar_fields(
    path: &Path,
    item_struct: &ItemStruct,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let raw_fields = matching_public_named_fields(
        fields,
        |name| looks_like_semantic_numeric_scalar_name(name, settings),
        type_is_integer_scalar,
    );
    if raw_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_semantic_numeric_scalar",
        format!(
            "public struct `{}` carries semantic scalar field(s) {} as raw integers; prefer typed duration, timestamp, port, or domain-specific scalar types",
            item_struct.ident,
            render_name_list(&raw_fields),
        ),
    ));
}

fn analyze_public_struct_key_value_bag_fields(
    path: &Path,
    item_struct: &ItemStruct,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let raw_fields = matching_public_named_fields(
        fields,
        |name| looks_like_key_value_bag_name(name, settings),
        type_is_string_key_value_bag_surface,
    );
    if raw_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_raw_key_value_bag",
        format!(
            "public struct `{}` exposes stringly key-value bag field(s) {}; prefer a typed metadata or options surface",
            item_struct.ident,
            render_name_list(&raw_fields),
        ),
    ));
}

fn analyze_public_struct_protocol_integer_fields(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let raw_fields = matching_public_named_fields(
        fields,
        looks_like_protocol_integer_name,
        type_is_integer_scalar,
    );
    if raw_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_integer_protocol_parameter",
        format!(
            "public struct `{}` carries protocol field(s) {} as raw integers; prefer typed enums or newtypes for boundary-facing protocol concepts",
            item_struct.ident,
            render_name_list(&raw_fields),
        ),
    ));
}

fn analyze_public_struct_raw_id_fields(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };
    let raw_fields =
        matching_public_named_fields(fields, looks_like_id_name, type_is_raw_id_scalar);
    if raw_fields.is_empty() {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_raw_id_surface",
        format!(
            "public struct `{}` keeps raw id field(s) {} as strings or primitive integers; prefer id newtypes at the boundary",
            item_struct.ident,
            render_name_list(&raw_fields),
        ),
    ));
}

fn analyze_public_enum_manual_display_surfaces(
    path: &Path,
    item_impl: &ItemImpl,
    public_enum_names: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((_, trait_path, _)) = &item_impl.trait_ else {
        return;
    };
    let Some(trait_name) = trait_path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    else {
        return;
    };
    if trait_name != "Display" {
        return;
    }

    let Some(enum_name) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };
    if !public_enum_names.contains(&enum_name) {
        return;
    }

    let Some(fmt_method) = item_impl.items.iter().find_map(|item| match item {
        ImplItem::Fn(method) if method.sig.ident == "fmt" => Some(method),
        _ => None,
    }) else {
        return;
    };
    if !display_impl_is_literal_string_match(fmt_method) {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_impl.span().start().line),
        "api_manual_enum_string_helper",
        format!(
            "public enum `{enum_name}` has a manual `Display` impl that only maps variants to string literals; prefer `strum::Display` when that string surface is canonical"
        ),
    ));
}

fn analyze_public_enum_strum_serialize_all_candidate(
    path: &Path,
    item_enum: &ItemEnum,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_enum.vis)
        || item_enum.variants.len() < 4
        || enum_has_strum_serialize_all(&item_enum.attrs)
    {
        return;
    }

    let mut variant_values = Vec::new();
    for variant in &item_enum.variants {
        let Some(value) = variant_single_strum_surface(variant) else {
            return;
        };
        variant_values.push((unraw_ident(&variant.ident), value));
    }

    for case in [
        StrumCase::KebabCase,
        StrumCase::SnakeCase,
        StrumCase::ScreamingSnakeCase,
        StrumCase::Lowercase,
    ] {
        if variant_values.iter().all(|(variant_name, value)| {
            render_strum_case(&split_segments(variant_name), case) == *value
        }) {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(item_enum.span().start().line),
                "api_strum_serialize_all_candidate",
                format!(
                    "enum `{}` repeats per-variant `strum` strings that match `{}`; prefer one enum-level `serialize_all` rule",
                    item_enum.ident,
                    strum_case_label(case),
                ),
            ));
            return;
        }
    }
}

fn analyze_public_struct_boolean_protocol_decisions(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };

    let item_name = unraw_ident(&item_struct.ident);
    for field in &fields.named {
        if !is_public(&field.vis) || !type_is_bool(&field.ty) {
            continue;
        }
        let Some(field_name) = field.ident.as_ref().map(unraw_ident) else {
            continue;
        };
        if !looks_like_protocol_decision_bool(&item_name, &field_name) {
            continue;
        }

        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(field.span().start().line),
            "api_boolean_protocol_decision",
            format!(
                "public boolean `{field_name}` encodes a protocol or domain decision on `{item_name}`; prefer a typed decision enum or small decision struct"
            ),
        ));
    }
}

fn analyze_public_fn_boolean_protocol_decisions(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_boolean_protocol_decisions_in_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        diagnostics,
    );
}

fn analyze_public_impl_boolean_protocol_decisions(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_boolean_protocol_decisions_in_signature(
            path,
            method.span().start().line,
            &method.sig.ident.to_string(),
            &method.sig.inputs,
            diagnostics,
        );
    }
}

fn analyze_boolean_protocol_decisions_in_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for input in inputs {
        let FnArg::Typed(arg) = input else {
            continue;
        };
        if !type_is_bool(&arg.ty) {
            continue;
        }
        let Some(param_name) = typed_arg_name(arg) else {
            continue;
        };
        if !looks_like_protocol_decision_bool(item_name, &param_name) {
            continue;
        }

        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_boolean_protocol_decision",
            format!(
                "boolean `{param_name}` encodes a protocol or domain decision in `{item_name}`; prefer a typed decision enum or small decision struct"
            ),
        ));
    }
}

fn analyze_public_fn_boolean_flag_clusters(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_boolean_flag_cluster_in_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        diagnostics,
    );
}

fn analyze_public_impl_boolean_flag_clusters(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_boolean_flag_cluster_in_signature(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.inputs,
            diagnostics,
        );
    }
}

fn analyze_boolean_flag_cluster_in_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bool_params = matching_named_inputs(inputs, |_| true, type_is_bool);
    if bool_params.len() < 2
        || bool_params
            .iter()
            .all(|name| looks_like_runtime_toggle_name(name))
    {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(line),
        "api_boolean_flag_cluster",
        format!(
            "public boundary `{item_name}` takes several boolean flags ({}); prefer typed modes, decisions, or an options surface when those flags jointly shape behavior",
            render_name_list(&bool_params),
        ),
    ));
}

fn analyze_public_fn_builder_candidate(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) || has_builder_attribute(&item_fn.attrs) {
        return;
    }

    analyze_builder_candidate_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        &item_fn.sig.output,
        Some(&item_fn.block),
        diagnostics,
    );
}

fn analyze_public_stringly_protocol_parameters(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_stringly_protocol_parameters_in_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        diagnostics,
    );
}

fn analyze_public_impl_stringly_protocol_parameters(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_stringly_protocol_parameters_in_signature(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.inputs,
            diagnostics,
        );
    }
}

fn analyze_stringly_protocol_parameters_in_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let stringly_params = inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(arg)
                if typed_arg_name(arg)
                    .as_deref()
                    .is_some_and(looks_like_protocol_descriptor_name)
                    && (type_is_string_input(&arg.ty) || type_is_string_field(&arg.ty)) =>
            {
                typed_arg_name(arg)
            }
            FnArg::Typed(_) | FnArg::Receiver(_) => None,
        })
        .collect::<Vec<_>>();
    if stringly_params.is_empty() {
        return;
    }

    let rendered = stringly_params
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(line),
        "api_stringly_protocol_parameter",
        format!(
            "public boundary `{item_name}` takes stringly protocol or state parameter(s) {rendered}; prefer typed enums or newtypes at the boundary"
        ),
    ));
}

fn analyze_public_fn_flag_bit_boundaries(
    path: &Path,
    item_fn: &ItemFn,
    flag_constants: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    let emitted_signature = analyze_flag_bit_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        &item_fn.sig.output,
        diagnostics,
    );
    if !emitted_signature {
        analyze_manual_flag_bit_patterns_in_block(
            path,
            item_fn.span().start().line,
            &item_fn.sig.ident.to_string(),
            &item_fn.block,
            flag_constants,
            diagnostics,
        );
    }
}

fn analyze_public_impl_flag_bit_boundaries(
    path: &Path,
    item_impl: &ItemImpl,
    scope_flag_constants: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };
    let mut flag_constants = scope_flag_constants.clone();
    flag_constants.extend(collect_impl_flag_constant_names(item_impl));

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        let item_name = format!("{self_type}::{}", method.sig.ident);
        let emitted_signature = analyze_flag_bit_signature(
            path,
            method.span().start().line,
            &item_name,
            &method.sig.inputs,
            &method.sig.output,
            diagnostics,
        );
        if !emitted_signature {
            analyze_manual_flag_bit_patterns_in_block(
                path,
                method.span().start().line,
                &item_name,
                &method.block,
                &flag_constants,
                diagnostics,
            );
        }
    }
}

fn analyze_flag_bit_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    output: &ReturnType,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut emitted = false;
    let raw_bits =
        matching_named_inputs(inputs, looks_like_flag_bits_name, type_is_bitflag_integer);
    if !raw_bits.is_empty() {
        emitted = true;
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_manual_flag_set",
            format!(
                "public boundary `{item_name}` uses raw bit-flag value(s) {}; prefer a typed flags boundary instead of manual `u32` or `u64` masks",
                render_name_list(&raw_bits),
            ),
        ));
    } else if looks_like_flag_bits_name(item_name)
        && return_type(output).is_some_and(type_is_bitflag_integer)
    {
        emitted = true;
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_manual_flag_set",
            format!(
                "public boundary `{item_name}` returns a raw bit-mask surface; prefer a typed flags boundary"
            ),
        ));
    }

    emitted
}

fn analyze_manual_flag_bit_patterns_in_block(
    path: &Path,
    default_line: usize,
    item_name: &str,
    block: &syn::Block,
    flag_constants: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if flag_constants.len() < 2 {
        return;
    }

    let uses = collect_flag_bit_pattern_uses(block, flag_constants);
    if uses.is_empty() {
        return;
    }

    let mut grouped =
        BTreeMap::<String, (usize, BTreeSet<String>, BTreeSet<BitMaskPatternKind>)>::new();
    for usage in uses {
        let entry = grouped
            .entry(usage.boundary_name)
            .or_insert_with(|| (usage.line, BTreeSet::new(), BTreeSet::new()));
        entry.0 = entry.0.min(usage.line);
        entry.1.insert(usage.const_name);
        entry.2.insert(usage.kind);
    }

    for (boundary_name, (line, const_names, kinds)) in grouped {
        if const_names.len() < 2 {
            continue;
        }

        let action = match (
            kinds.contains(&BitMaskPatternKind::Check),
            kinds.contains(&BitMaskPatternKind::Assembly),
        ) {
            (true, true) => "checks and assembles",
            (true, false) => "checks",
            (false, true) => "assembles",
            (false, false) => "manipulates",
        };
        let rendered_consts = const_names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line.max(default_line)),
            "api_manual_flag_set",
            format!(
                "public boundary `{item_name}` {action} raw bit flags on `{boundary_name}` with named flags {rendered_consts}; prefer a typed flags boundary instead of manual bitmask logic"
            ),
        ));
    }
}

fn collect_flag_bit_pattern_uses(
    block: &syn::Block,
    flag_constants: &BTreeSet<String>,
) -> Vec<FlagBitPatternUse> {
    let mut visitor = FlagBitPatternVisitor {
        flag_constants,
        uses: Vec::new(),
    };
    visitor.visit_block(block);
    visitor.uses
}

impl<'ast> Visit<'ast> for FlagBitPatternVisitor<'_> {
    fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
        if let Some(kind) = bitmask_pattern_kind(&node.op) {
            let boundaries = collect_bitmask_boundary_names(node.left.as_ref())
                .into_iter()
                .chain(collect_bitmask_boundary_names(node.right.as_ref()))
                .collect::<BTreeSet<_>>();
            let const_names =
                collect_bitmask_flag_constant_names(node.left.as_ref(), self.flag_constants)
                    .into_iter()
                    .chain(collect_bitmask_flag_constant_names(
                        node.right.as_ref(),
                        self.flag_constants,
                    ))
                    .collect::<BTreeSet<_>>();

            if !boundaries.is_empty() && !const_names.is_empty() {
                let line = node.span().start().line;
                for boundary_name in boundaries {
                    for const_name in &const_names {
                        self.uses.push(FlagBitPatternUse {
                            boundary_name: boundary_name.clone(),
                            const_name: const_name.clone(),
                            kind,
                            line,
                        });
                    }
                }
            }
        }

        syn::visit::visit_expr_binary(self, node);
    }
}

fn bitmask_pattern_kind(op: &BinOp) -> Option<BitMaskPatternKind> {
    match op {
        BinOp::BitAnd(_) | BinOp::BitAndAssign(_) => Some(BitMaskPatternKind::Check),
        BinOp::BitOr(_) | BinOp::BitOrAssign(_) => Some(BitMaskPatternKind::Assembly),
        _ => None,
    }
}

fn collect_bitmask_boundary_names(expr: &Expr) -> BTreeSet<String> {
    match expr {
        Expr::Binary(binary) if bitmask_pattern_kind(&binary.op).is_some() => {
            collect_bitmask_boundary_names(binary.left.as_ref())
                .into_iter()
                .chain(collect_bitmask_boundary_names(binary.right.as_ref()))
                .collect()
        }
        Expr::Field(field) => match &field.member {
            syn::Member::Named(ident) => {
                let name = unraw_ident(ident);
                looks_like_flag_bits_name(&name)
                    .then_some(name)
                    .into_iter()
                    .collect()
            }
            syn::Member::Unnamed(_) => BTreeSet::new(),
        },
        _ => expr_path_leaf_ident(expr)
            .filter(|name| looks_like_flag_bits_name(name))
            .into_iter()
            .collect(),
    }
}

fn collect_bitmask_flag_constant_names(
    expr: &Expr,
    flag_constants: &BTreeSet<String>,
) -> BTreeSet<String> {
    match expr {
        Expr::Binary(binary) if bitmask_pattern_kind(&binary.op).is_some() => {
            collect_bitmask_flag_constant_names(binary.left.as_ref(), flag_constants)
                .into_iter()
                .chain(collect_bitmask_flag_constant_names(
                    binary.right.as_ref(),
                    flag_constants,
                ))
                .collect()
        }
        _ => expr_path_leaf_ident(expr)
            .filter(|name| flag_constants.contains(name))
            .into_iter()
            .collect(),
    }
}

fn analyze_public_fn_typed_error_surfaces(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_typed_error_output(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.output,
        Some(&item_fn.block),
        diagnostics,
    );
}

fn analyze_public_impl_typed_error_surfaces(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_typed_error_output(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.output,
            Some(&method.block),
            diagnostics,
        );
    }
}

fn analyze_typed_error_output(
    path: &Path,
    line: usize,
    item_name: &str,
    output: &ReturnType,
    body: Option<&syn::Block>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(output_ty) = return_type(output) else {
        return;
    };

    if type_is_result_string_error(output_ty)
        && !body.is_some_and(block_is_string_parse_passthrough)
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_string_error_surface",
            format!(
                "public boundary `{item_name}` returns `Result<_, String>` or another raw string error surface; prefer a typed error boundary"
            ),
        ));
    }

    if type_is_anyhow_result_or_error(output_ty) {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_anyhow_error_surface",
            format!(
                "public boundary `{item_name}` exposes `anyhow` in its error surface; keep `anyhow` internal and return a crate-owned typed error boundary"
            ),
        ));
    }
}

fn block_is_string_parse_passthrough(block: &syn::Block) -> bool {
    let [Stmt::Expr(expr, _)] = block.stmts.as_slice() else {
        return false;
    };
    expr_is_string_parse_passthrough(expr)
}

fn expr_is_string_parse_passthrough(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(method)
            if method.method == "parse"
                && method.args.is_empty()
                && expr_is_single_ident_path(&method.receiver) =>
        {
            true
        }
        Expr::Call(call)
            if call.args.len() == 1
                && call.args.first().is_some_and(expr_is_single_ident_path)
                && call_target_is_parse_helper(&call.func) =>
        {
            true
        }
        Expr::Block(block) => block_is_string_parse_passthrough(&block.block),
        _ => false,
    }
}

fn call_target_is_parse_helper(func: &Expr) -> bool {
    let Expr::Path(path) = func else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "parse")
}

fn expr_is_single_ident_path(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    path.path.segments.len() == 1
}

fn expr_is_option_some_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call)
            if call.args.len() == 1 && expr_is_option_some_constructor(call.func.as_ref()) =>
        {
            true
        }
        Expr::Group(group) => expr_is_option_some_call(&group.expr),
        Expr::Paren(paren) => expr_is_option_some_call(&paren.expr),
        _ => false,
    }
}

fn expr_is_option_some_constructor(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    let mut segments = path.path.segments.iter();
    let Some(last) = segments.next_back() else {
        return false;
    };
    if last.ident != "Some" {
        return false;
    }

    path.path.segments.len() == 1 || segments.any(|segment| segment.ident == "Option")
}

fn analyze_public_fn_semantic_scalar_boundaries(
    path: &Path,
    item_fn: &ItemFn,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_semantic_scalar_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        &item_fn.sig.output,
        settings,
        diagnostics,
    );
}

fn analyze_public_impl_semantic_scalar_boundaries(
    path: &Path,
    item_impl: &ItemImpl,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_semantic_scalar_signature(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.inputs,
            &method.sig.output,
            settings,
            diagnostics,
        );
    }
}

fn analyze_semantic_scalar_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    output: &ReturnType,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let string_scalars = matching_named_inputs(
        inputs,
        |name| {
            looks_like_semantic_string_scalar_name(name, settings)
                && !looks_like_protocol_descriptor_name(name)
        },
        type_is_string_input,
    );
    if !string_scalars.is_empty() {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_semantic_string_scalar",
            format!(
                "public boundary `{item_name}` uses semantic scalar(s) {} as raw strings; prefer typed boundary values or focused newtypes",
                render_name_list(&string_scalars),
            ),
        ));
    } else if looks_like_semantic_string_scalar_name(item_name, settings)
        && !looks_like_render_or_format_helper(item_name)
        && return_type(output).is_some_and(type_is_string_surface)
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_semantic_string_scalar",
            format!(
                "public boundary `{item_name}` returns a semantic scalar as a raw string; prefer a typed boundary value or focused newtype"
            ),
        ));
    }

    let numeric_scalars = matching_named_inputs(
        inputs,
        |name| looks_like_semantic_numeric_scalar_name(name, settings),
        type_is_integer_scalar,
    );
    if !numeric_scalars.is_empty() {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_semantic_numeric_scalar",
            format!(
                "public boundary `{item_name}` uses semantic scalar(s) {} as raw integers; prefer typed duration, timestamp, port, or domain-specific scalar types",
                render_name_list(&numeric_scalars),
            ),
        ));
    } else if looks_like_semantic_numeric_scalar_name(item_name, settings)
        && !looks_like_render_or_format_helper(item_name)
        && return_type(output).is_some_and(type_is_integer_scalar)
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_semantic_numeric_scalar",
            format!(
                "public boundary `{item_name}` returns a semantic scalar as a raw integer; prefer a typed duration, timestamp, port, or domain-specific scalar type"
            ),
        ));
    }
}

fn analyze_public_fn_key_value_bag_surfaces(
    path: &Path,
    item_fn: &ItemFn,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_key_value_bag_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        &item_fn.sig.output,
        settings,
        diagnostics,
    );
}

fn analyze_public_impl_key_value_bag_surfaces(
    path: &Path,
    item_impl: &ItemImpl,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_key_value_bag_signature(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.inputs,
            &method.sig.output,
            settings,
            diagnostics,
        );
    }
}

fn analyze_key_value_bag_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    output: &ReturnType,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bag_params = matching_named_inputs(
        inputs,
        |name| looks_like_key_value_bag_name(name, settings),
        type_is_string_key_value_bag_surface,
    );
    if !bag_params.is_empty() {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_raw_key_value_bag",
            format!(
                "public boundary `{item_name}` exposes stringly key-value bag(s) {}; prefer a typed metadata or options surface",
                render_name_list(&bag_params),
            ),
        ));
    } else if looks_like_key_value_bag_name(item_name, settings)
        && !looks_like_render_or_format_helper(item_name)
        && return_type(output).is_some_and(type_is_string_key_value_bag_surface)
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_raw_key_value_bag",
            format!(
                "public boundary `{item_name}` returns a stringly key-value bag; prefer a typed metadata or options surface"
            ),
        ));
    }
}

fn analyze_public_fn_protocol_integer_boundaries(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_protocol_integer_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        &item_fn.sig.output,
        diagnostics,
    );
}

fn analyze_public_impl_protocol_integer_boundaries(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_protocol_integer_signature(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.inputs,
            &method.sig.output,
            diagnostics,
        );
    }
}

fn analyze_protocol_integer_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    output: &ReturnType,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let params = matching_named_inputs(
        inputs,
        looks_like_protocol_integer_name,
        type_is_integer_scalar,
    );
    if !params.is_empty() {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_integer_protocol_parameter",
            format!(
                "public boundary `{item_name}` uses protocol parameter(s) {} as raw integers; prefer typed enums or newtypes for boundary-facing protocol concepts",
                render_name_list(&params),
            ),
        ));
    } else if looks_like_protocol_integer_name(item_name)
        && return_type(output).is_some_and(type_is_integer_scalar)
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_integer_protocol_parameter",
            format!(
                "public boundary `{item_name}` returns a protocol concept as a raw integer; prefer a typed enum or newtype"
            ),
        ));
    }
}

fn analyze_public_fn_raw_id_boundaries(
    path: &Path,
    item_fn: &ItemFn,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    analyze_raw_id_signature(
        path,
        item_fn.span().start().line,
        &item_fn.sig.ident.to_string(),
        &item_fn.sig.inputs,
        &item_fn.sig.output,
        diagnostics,
    );
}

fn analyze_public_impl_raw_id_boundaries(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) {
            continue;
        }

        analyze_raw_id_signature(
            path,
            method.span().start().line,
            &format!("{self_type}::{}", method.sig.ident),
            &method.sig.inputs,
            &method.sig.output,
            diagnostics,
        );
    }
}

fn analyze_raw_id_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    output: &ReturnType,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let raw_ids = matching_named_inputs(inputs, looks_like_id_name, type_is_raw_id_scalar);
    if !raw_ids.is_empty() {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_raw_id_surface",
            format!(
                "public boundary `{item_name}` uses raw id value(s) {}; prefer typed id newtypes at the boundary",
                render_name_list(&raw_ids),
            ),
        ));
    } else if looks_like_id_name(item_name)
        && return_type(output).is_some_and(type_is_raw_id_scalar)
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_raw_id_surface",
            format!(
                "public boundary `{item_name}` returns a raw id value; prefer a typed id newtype at the boundary"
            ),
        ));
    }
}

fn analyze_public_type_alias_error_surfaces(
    path: &Path,
    item_type: &ItemType,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_type.vis) {
        return;
    }

    if type_is_result_string_error(item_type.ty.as_ref()) {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(item_type.span().start().line),
            "api_string_error_surface",
            format!(
                "public type alias `{}` keeps a raw string error surface; prefer a typed error boundary",
                item_type.ident
            ),
        ));
    }

    if type_is_anyhow_result_or_error(item_type.ty.as_ref()) {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(item_type.span().start().line),
            "api_anyhow_error_surface",
            format!(
                "public type alias `{}` exposes `anyhow` in the caller-facing error surface; keep `anyhow` internal and expose a typed crate-owned error boundary",
                item_type.ident
            ),
        ));
    }
}

fn analyze_public_type_alias_raw_id_surface(
    path: &Path,
    item_type: &ItemType,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_type.vis)
        || !looks_like_id_name(&item_type.ident.to_string())
        || !type_is_raw_id_scalar(item_type.ty.as_ref())
    {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_type.span().start().line),
        "api_raw_id_surface",
        format!(
            "public type alias `{}` keeps an id as a raw string or primitive integer; prefer a typed id newtype",
            item_type.ident
        ),
    ));
}

fn analyze_public_type_alias_key_value_bag_surface(
    path: &Path,
    item_type: &ItemType,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_type.vis)
        || !looks_like_key_value_bag_name(&item_type.ident.to_string(), settings)
        || !type_is_string_key_value_bag_surface(item_type.ty.as_ref())
    {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_type.span().start().line),
        "api_raw_key_value_bag",
        format!(
            "public type alias `{}` keeps a stringly key-value bag at the boundary; prefer a typed metadata or options surface",
            item_type.ident
        ),
    ));
}

fn analyze_public_impl_builder_candidates(
    path: &Path,
    item_impl: &ItemImpl,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) || has_builder_attribute(&method.attrs) {
            continue;
        }

        analyze_builder_candidate_signature(
            path,
            method.span().start().line,
            &method.sig.ident.to_string(),
            &method.sig.inputs,
            &method.sig.output,
            Some(&method.block),
            diagnostics,
        );
    }
}

fn analyze_builder_candidate_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    output: &ReturnType,
    block: Option<&syn::Block>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let typed_inputs = typed_inputs(inputs);
    if let Some(block) = block
        && analyze_defaulted_optional_parameter_signature(
            path,
            line,
            item_name,
            &typed_inputs,
            block,
            diagnostics,
        )
    {
        return;
    }
    if analyze_optional_parameter_builder_signature(
        path,
        line,
        item_name,
        &typed_inputs,
        diagnostics,
    ) {
        return;
    }
    if typed_inputs.len() < 3 {
        return;
    }

    let builderish_name = is_builderish_function_name(item_name);
    let returns_constructed = output_returns_constructed_type(output);
    if !builderish_name && !returns_constructed {
        return;
    }

    let weak_param_count = typed_inputs
        .iter()
        .filter(|(_, ty)| is_weak_builder_param_type(ty))
        .count();
    let duplicate_type_names = repeated_simplified_type_names(&typed_inputs);
    let should_flag = if typed_inputs.len() >= 4 {
        builderish_name || weak_param_count >= 1 || !duplicate_type_names.is_empty()
    } else {
        weak_param_count >= 2 || !duplicate_type_names.is_empty()
    };
    if !should_flag {
        return;
    }

    let param_names = typed_inputs
        .iter()
        .filter_map(|(name, _)| name.clone())
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::policy(
        Some(path.to_path_buf()),
        Some(line),
        "api_builder_candidate",
        format!(
            "public entrypoint `{item_name}` takes {} positional parameters ({param_names}); prefer a builder or typed options struct when setup is this configuration-heavy",
            typed_inputs.len()
        ),
    ));
}

fn analyze_defaulted_optional_parameter_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    typed_inputs: &[(Option<String>, &Type)],
    block: &syn::Block,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if typed_inputs.len() < 2 || !is_builderish_function_name(item_name) {
        return false;
    }

    let defaulted_params = defaulted_optional_params(block, typed_inputs);
    if defaulted_params.is_empty() {
        return false;
    }

    let param_names = defaulted_params
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::policy(
        Some(path.to_path_buf()),
        Some(line),
        "api_defaulted_optional_parameter",
        format!(
            "public entrypoint `{item_name}` immediately defaults optional positional parameters ({param_names}); prefer a `bon` builder so callers can omit defaulted values instead of passing `None`"
        ),
    ));
    true
}

fn analyze_optional_parameter_builder_signature(
    path: &Path,
    line: usize,
    item_name: &str,
    typed_inputs: &[(Option<String>, &Type)],
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if typed_inputs.len() < 2 || !is_builderish_function_name(item_name) {
        return false;
    }

    let optional_params = typed_inputs
        .iter()
        .filter(|(_, ty)| type_is_option_surface(ty))
        .filter_map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if optional_params.is_empty() {
        return false;
    }

    let param_names = optional_params
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::policy(
        Some(path.to_path_buf()),
        Some(line),
        "api_optional_parameter_builder",
        format!(
            "public entrypoint `{item_name}` takes optional positional parameters ({param_names}); prefer a `bon` builder so callers can omit unset values instead of passing `None`"
        ),
    ));
    true
}

fn defaulted_optional_params(
    block: &syn::Block,
    typed_inputs: &[(Option<String>, &Type)],
) -> Vec<String> {
    let option_params = typed_inputs
        .iter()
        .filter(|(_, ty)| type_is_option_surface(ty))
        .filter_map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    if option_params.is_empty() {
        return Vec::new();
    }

    let mut visitor = DefaultedOptionParamVisitor {
        option_params: &option_params,
        defaulted_params: BTreeSet::new(),
    };
    visitor.visit_block(block);
    visitor.defaulted_params.into_iter().collect()
}

struct DefaultedOptionParamVisitor<'a> {
    option_params: &'a BTreeSet<String>,
    defaulted_params: BTreeSet<String>,
}

struct MaybeSomeMethodCallVisitor<'a> {
    path: &'a Path,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'ast> Visit<'ast> for DefaultedOptionParamVisitor<'_> {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if matches!(
            node.method.to_string().as_str(),
            "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" | "map_or"
        ) && let Some(param_name) = expr_simple_path_ident(node.receiver.as_ref())
            && self.option_params.contains(&param_name)
        {
            self.defaulted_params.insert(param_name);
        }

        syn::visit::visit_expr_method_call(self, node);
    }
}

impl<'ast> Visit<'ast> for MaybeSomeMethodCallVisitor<'_> {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method_name = node.method.to_string();
        if let Some(setter_name) = method_name.strip_prefix("maybe_")
            && !setter_name.is_empty()
            && node.args.len() == 1
            && node.args.first().is_some_and(expr_is_option_some_call)
        {
            self.diagnostics.push(Diagnostic::advisory(
                Some(self.path.to_path_buf()),
                Some(node.span().start().line),
                "callsite_maybe_some",
                format!(
                    "call `{method_name}(Some(...))` wraps a concrete value just to feed a `maybe_` setter; pass an `Option<_>` value to `{method_name}` or call `{setter_name}(...)` when you already have the value"
                ),
            ));
        }

        syn::visit::visit_expr_method_call(self, node);
    }
}

fn repeated_parameter_cluster_signature(
    item_name: &str,
    typed_inputs: &[(Option<String>, &Type)],
    output: &ReturnType,
) -> Option<Vec<(String, String)>> {
    if typed_inputs.len() < 3 {
        return None;
    }
    if !is_builderish_function_name(item_name) && !output_returns_constructed_type(output) {
        return None;
    }
    let has_weak_param = typed_inputs
        .iter()
        .any(|(_, ty)| is_weak_builder_param_type(ty));
    if typed_inputs.len() < 4 && !has_weak_param {
        return None;
    }

    typed_inputs
        .iter()
        .map(|(name, ty)| Some((name.clone()?, type_cluster_fingerprint(ty)?)))
        .collect()
}

fn analyze_standalone_builder_surfaces(
    path: &Path,
    items: &[Item],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut families = BTreeMap::<String, Vec<(usize, String)>>::new();

    for item in items {
        let Item::Fn(item_fn) = item else {
            continue;
        };
        if !is_public(&item_fn.vis) {
            continue;
        }

        let function_name = item_fn.sig.ident.to_string();
        if !is_standalone_builder_function_name(&function_name) {
            continue;
        }
        let Some(subject_type) = standalone_builder_subject_type(item_fn) else {
            continue;
        };
        families
            .entry(subject_type)
            .or_default()
            .push((item_fn.span().start().line, function_name));
    }

    for (subject_type, functions) in families {
        if functions.len() < 3 {
            continue;
        }

        let line = functions
            .iter()
            .map(|(line, _)| *line)
            .min()
            .expect("non-empty");
        let function_names = functions
            .iter()
            .map(|(_, name)| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_standalone_builder_surface",
            format!(
                "public free functions {function_names} look like a standalone builder surface for `{subject_type}`; prefer inherent setters or a builder instead of parallel `with_*` helpers"
            ),
        ));
    }
}

fn analyze_forwarding_compat_wrappers(
    path: &Path,
    item_impl: &ItemImpl,
    trait_surfaces: &TraitSurfaceCatalog,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(self_type) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if !is_public(&method.vis) || method.sig.inputs.len() != 1 {
            continue;
        }

        let Some(expr) = block_single_expr(&method.block) else {
            continue;
        };
        let helper_name = method.sig.ident.to_string();
        if !is_explicit_from_wrapper_name(&helper_name, &method.sig.output) {
            continue;
        }

        if let Some(target_type) = output_type_leaf_ident(&method.sig.output)
            && trait_surfaces
                .from_targets_by_source
                .get(&self_type)
                .is_some_and(|targets| targets.contains(&target_type))
            && expr_is_direct_from_forward(expr, &target_type)
        {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(method.span().start().line),
                "api_forwarding_compat_wrapper",
                format!(
                    "public method `{helper_name}` only forwards to existing `From<{self_type}> for {target_type}` conversion; prefer using the trait surface directly"
                ),
            ));
            continue;
        }
    }
}

fn analyze_public_enum_impl_string_helpers(
    path: &Path,
    item_impl: &ItemImpl,
    public_enum_names: &BTreeSet<String>,
    from_str_targets: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item_impl.trait_.is_some() {
        return;
    }

    let Some(enum_name) = type_leaf_ident(&item_impl.self_ty) else {
        return;
    };
    if !public_enum_names.contains(&enum_name) {
        return;
    }

    let helper_names = item_impl
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) => manual_enum_string_helper_name(method),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if !helper_names.is_empty() {
        let helpers = helper_names
            .into_iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(item_impl.span().start().line),
            "api_manual_enum_string_helper",
            format!(
                "public enum `{enum_name}` exposes manual string helper(s) {helpers}; prefer a standard string surface such as `Display`/`AsRefStr`, and `EnumString` if parsing is canonical"
            ),
        ));
    }

    let metadata_helpers = item_impl
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) => enum_metadata_helper(method).map(|kind| {
                (
                    method.sig.ident.to_string(),
                    matches!(kind, MetadataHelperKind::StringSurface),
                )
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    let string_helpers = metadata_helpers
        .iter()
        .filter(|(_, is_string)| *is_string)
        .count();
    let typed_helpers = metadata_helpers.len() - string_helpers;
    let strong_metadata_helpers = metadata_helpers
        .iter()
        .filter(|(name, _)| looks_like_enum_metadata_helper_name(name))
        .count();
    if (metadata_helpers.len() >= 2 && strong_metadata_helpers >= 2)
        || (metadata_helpers.len() >= 3 && (string_helpers >= 2 || typed_helpers >= 1))
    {
        let helpers = metadata_helpers
            .iter()
            .map(|(name, _)| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(item_impl.span().start().line),
            "api_parallel_enum_metadata_helper",
            format!(
                "public enum `{enum_name}` exposes parallel metadata helpers {helpers}; prefer a typed descriptor surface instead of repeating separate matches for each metadata view"
            ),
        ));
    }

    let parse_methods = item_impl
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) => manual_enum_parse_helper_name(method, &enum_name),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if parse_methods.is_empty() || from_str_targets.contains(&enum_name) {
        return;
    }

    let helpers = parse_methods
        .into_iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_impl.span().start().line),
        "api_ad_hoc_parse_helper",
        format!(
            "public enum `{enum_name}` exposes ad hoc parse helper(s) {helpers}; prefer `FromStr` or `TryFrom<&str>` on `{enum_name}` when string parsing is the canonical boundary"
        ),
    ));
}

fn manual_enum_string_helper_name(method: &ImplItemFn) -> Option<String> {
    if !is_public(&method.vis) || method.sig.constness.is_some() || method.sig.inputs.len() != 1 {
        return None;
    }
    if !matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_))) {
        return None;
    }
    if !returns_string_surface(&method.sig.output) {
        return None;
    }

    let helper_name = method.sig.ident.to_string();
    matches!(helper_name.as_str(), "as_str" | "label" | "to_str").then_some(helper_name)
}

fn analyze_public_enum_string_helper_functions(
    path: &Path,
    item_fn: &ItemFn,
    public_enum_names: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) || !returns_string_surface(&item_fn.sig.output) {
        return;
    }

    let helper_name = item_fn.sig.ident.to_string();
    if !is_enum_string_helper_function_name(&helper_name) {
        return;
    }
    let Some(target_enum) = single_public_enum_input(&item_fn.sig.inputs, public_enum_names) else {
        return;
    };

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_fn.span().start().line),
        "api_manual_enum_string_helper",
        format!(
            "public helper `{helper_name}` renders `{target_enum}` as a string; prefer a standard string surface on `{target_enum}` such as `Display`/`AsRefStr`"
        ),
    ));
}

fn analyze_public_parse_helpers(
    path: &Path,
    item_fn: &ItemFn,
    public_enum_names: &BTreeSet<String>,
    from_str_targets: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_fn.vis) {
        return;
    }

    let helper_name = item_fn.sig.ident.to_string();
    if !helper_name.starts_with("parse_") || !has_single_string_input(&item_fn.sig.inputs) {
        return;
    }

    let Some(target_enum) = returned_public_enum_name(&item_fn.sig.output, public_enum_names)
    else {
        return;
    };
    if from_str_targets.contains(&target_enum) {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_fn.span().start().line),
        "api_ad_hoc_parse_helper",
        format!(
            "public helper `{helper_name}` parses strings into `{target_enum}`; prefer `FromStr` or `TryFrom<&str>` on `{target_enum}` when the parse is the canonical boundary"
        ),
    ));
}

fn analyze_public_stringly_model_scaffolds(
    path: &Path,
    item_struct: &ItemStruct,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_struct.vis) {
        return;
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return;
    };

    let string_fields = fields
        .named
        .iter()
        .filter(|field| is_public(&field.vis))
        .filter_map(string_field_signal)
        .collect::<Vec<_>>();
    if string_fields.len() < 3 {
        return;
    }

    let scaffold_fields = string_fields
        .iter()
        .filter(|field| field.scaffold_like)
        .collect::<Vec<_>>();
    if scaffold_fields.len() < 2
        || !scaffold_fields
            .iter()
            .any(|field| field.has_strong_metadata_token)
    {
        return;
    }

    let field_names = scaffold_fields
        .iter()
        .map(|field| format!("`{}`", field.name))
        .collect::<Vec<_>>()
        .join(", ");
    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(item_struct.span().start().line),
        "api_stringly_model_scaffold",
        format!(
            "public struct `{}` carries semantic descriptor fields as raw strings ({field_names}); this looks like string-heavy model scaffolding. Prefer typed enums, newtypes, or a focused descriptor type and render strings only at the boundary",
            item_struct.ident
        ),
    ));
}

fn analyze_public_stringly_protocol_collection_const(
    path: &Path,
    item_const: &ItemConst,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_const.vis) {
        return;
    }

    analyze_stringly_protocol_collection_binding(
        path,
        item_const.span().start().line,
        &item_const.ident.to_string(),
        &item_const.ty,
        diagnostics,
    );
}

fn analyze_public_stringly_protocol_collection_static(
    path: &Path,
    item_static: &ItemStatic,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_public(&item_static.vis) {
        return;
    }

    analyze_stringly_protocol_collection_binding(
        path,
        item_static.span().start().line,
        &item_static.ident.to_string(),
        &item_static.ty,
        diagnostics,
    );
}

fn analyze_stringly_protocol_collection_binding(
    path: &Path,
    line: usize,
    binding_name: &str,
    ty: &Type,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !type_is_string_collection_surface(ty)
        || !collection_name_suggests_protocol_model(binding_name)
    {
        return;
    }

    diagnostics.push(Diagnostic::advisory(
        Some(path.to_path_buf()),
        Some(line),
        "api_stringly_protocol_collection",
        format!(
            "public collection `{binding_name}` stores protocol or state values as raw strings; prefer typed enums, newtypes, or a typed descriptor map and render strings only at the boundary"
        ),
    ));
}

fn enum_metadata_helper(method: &ImplItemFn) -> Option<MetadataHelperKind> {
    if !is_public(&method.vis) || method.sig.constness.is_some() || method.sig.inputs.len() != 1 {
        return None;
    }
    if !matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_))) {
        return None;
    }
    if !looks_like_enum_metadata_helper_name(&method.sig.ident.to_string()) {
        return None;
    }
    if !body_is_match_on_self(&method.block) {
        return None;
    }

    if returns_string_surface(&method.sig.output) {
        return Some(MetadataHelperKind::StringSurface);
    }

    returns_metadata_surface(&method.sig.output).then_some(MetadataHelperKind::TypedSurface)
}

fn looks_like_enum_metadata_helper_name(name: &str) -> bool {
    let tokens = name_tokens(name);
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "code" | "label" | "description" | "color" | "icon" | "tag" | "name" | "status"
        )
    })
}

fn manual_enum_parse_helper_name(method: &ImplItemFn, enum_name: &str) -> Option<String> {
    if !is_public(&method.vis) || method.sig.constness.is_some() || method.sig.inputs.len() != 1 {
        return None;
    }
    if matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_))) {
        return None;
    }
    if !has_single_string_input(&method.sig.inputs)
        || !returns_named_enum_or_self(&method.sig.output, enum_name)
    {
        return None;
    }

    let helper_name = method.sig.ident.to_string();
    (helper_name == "parse"
        || helper_name.starts_with("parse_")
        || helper_name.starts_with("from_"))
    .then_some(helper_name)
}

fn has_single_string_input(inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>) -> bool {
    if inputs.len() != 1 {
        return false;
    }

    match inputs.first() {
        Some(FnArg::Typed(arg)) => type_is_string_input(&arg.ty),
        Some(FnArg::Receiver(_)) | None => false,
    }
}

fn returned_public_enum_name(
    output: &ReturnType,
    public_enum_names: &BTreeSet<String>,
) -> Option<String> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    let returned = returned_type_leaf_ident(ty)?;
    public_enum_names.contains(&returned).then_some(returned)
}

fn returns_named_enum_or_self(output: &ReturnType, enum_name: &str) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };

    returned_type_leaf_ident(ty).is_some_and(|returned| returned == enum_name || returned == "Self")
}

fn returns_metadata_surface(output: &ReturnType) -> bool {
    if returns_string_surface(output) {
        return true;
    }

    let Some(leaf) = output_type_leaf_ident(output) else {
        return false;
    };
    !is_obvious_scalar_type_name(&leaf) && leaf != "Self"
}

fn returned_type_leaf_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last()?;
            if matches!(segment.ident.to_string().as_str(), "Result" | "Option") {
                let PathArguments::AngleBracketed(args) = &segment.arguments else {
                    return None;
                };
                let first = args.args.iter().find_map(|arg| match arg {
                    GenericArgument::Type(ty) => Some(ty),
                    _ => None,
                })?;
                return type_leaf_ident(first);
            }

            type_leaf_ident(ty)
        }
        _ => None,
    }
}

fn returns_string_surface(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };

    type_is_string_surface(ty)
}

fn type_is_string_surface(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            type_leaf_ident(&reference.elem).is_some_and(|ident| ident == "str")
        }
        Type::Path(_) => type_leaf_ident(ty).is_some_and(|ident| ident == "String"),
        _ => false,
    }
}

fn type_is_string_input(ty: &Type) -> bool {
    matches!(ty, Type::Reference(_) | Type::Path(_)) && type_is_string_surface(ty)
}

fn type_is_bool(ty: &Type) -> bool {
    borrowed_or_owned_type_leaf_ident(ty).is_some_and(|ident| ident == "bool")
}

fn type_is_integer_scalar(ty: &Type) -> bool {
    borrowed_or_owned_type_leaf_ident(ty).is_some_and(|ident| {
        matches!(
            ident.as_str(),
            "u8" | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
        )
    })
}

fn type_is_bitflag_integer(ty: &Type) -> bool {
    borrowed_or_owned_type_leaf_ident(ty)
        .is_some_and(|ident| matches!(ident.as_str(), "u32" | "u64"))
}

fn type_is_raw_id_scalar(ty: &Type) -> bool {
    type_is_string_input(ty) || type_is_string_field(ty) || type_is_integer_scalar(ty)
}

fn return_type(output: &ReturnType) -> Option<&Type> {
    match output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(ty.as_ref()),
    }
}

fn single_public_enum_input(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    public_enum_names: &BTreeSet<String>,
) -> Option<String> {
    if inputs.len() != 1 {
        return None;
    }

    match inputs.first() {
        Some(FnArg::Typed(arg)) => {
            let enum_name = borrowed_or_owned_type_leaf_ident(&arg.ty)?;
            public_enum_names.contains(&enum_name).then_some(enum_name)
        }
        Some(FnArg::Receiver(_)) | None => None,
    }
}

fn is_enum_string_helper_function_name(name: &str) -> bool {
    name.ends_with("_label") || name.ends_with("_name") || name.ends_with("_str")
}

fn type_leaf_ident(ty: &Type) -> Option<String> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    type_path
        .qself
        .is_none()
        .then(|| {
            type_path
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
        })
        .flatten()
}

fn borrowed_or_owned_type_leaf_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference(reference) => borrowed_or_owned_type_leaf_ident(&reference.elem),
        _ => type_leaf_ident(ty),
    }
}

fn output_type_leaf_ident(output: &ReturnType) -> Option<String> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };

    returned_type_leaf_ident(ty)
}

fn typed_inputs(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
) -> Vec<(Option<String>, &Type)> {
    inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(arg) => Some((typed_arg_name(arg), arg.ty.as_ref())),
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn typed_arg_name(arg: &syn::PatType) -> Option<String> {
    let syn::Pat::Ident(ident) = arg.pat.as_ref() else {
        return None;
    };

    Some(unraw_ident(&ident.ident))
}

fn matching_named_inputs(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    name_pred: impl Fn(&str) -> bool,
    type_pred: impl Fn(&Type) -> bool,
) -> Vec<String> {
    inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(arg) => {
                let name = typed_arg_name(arg)?;
                (name_pred(&name) && type_pred(&arg.ty)).then_some(name)
            }
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn matching_public_named_fields(
    fields: &syn::FieldsNamed,
    name_pred: impl Fn(&str) -> bool,
    type_pred: impl Fn(&Type) -> bool,
) -> Vec<String> {
    fields
        .named
        .iter()
        .filter(|field| is_public(&field.vis))
        .filter_map(|field| {
            let name = field.ident.as_ref().map(unraw_ident)?;
            (name_pred(&name) && type_pred(&field.ty)).then_some(name)
        })
        .collect()
}

fn render_name_list(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_builderish_function_name(name: &str) -> bool {
    matches!(
        name,
        "new" | "start" | "open" | "create" | "build" | "run" | "entry"
    ) || matches!(
        name,
        _
            if name.starts_with("build_")
                || name.starts_with("start_")
                || name.starts_with("create_")
                || name.starts_with("open_")
    )
}

fn output_returns_constructed_type(output: &ReturnType) -> bool {
    let Some(type_name) = output_type_leaf_ident(output) else {
        return false;
    };

    type_name == "Self" || !is_obvious_scalar_type_name(&type_name)
}

fn is_weak_builder_param_type(ty: &Type) -> bool {
    type_is_bool(ty)
        || type_is_option_surface(ty)
        || type_is_string_surface(ty)
        || type_is_string_field(ty)
        || borrowed_or_owned_type_leaf_ident(ty)
            .is_some_and(|type_name| is_obvious_scalar_type_name(&type_name))
}

fn type_is_option_surface(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => type_is_option_surface(&reference.elem),
        Type::Path(_) => type_leaf_ident(ty).is_some_and(|ident| ident == "Option"),
        _ => false,
    }
}

fn type_cluster_fingerprint(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference(reference) => {
            Some(format!("&{}", type_cluster_fingerprint(&reference.elem)?))
        }
        Type::Path(type_path) => {
            let mut parts = Vec::new();
            for segment in &type_path.path.segments {
                let mut part = segment.ident.to_string();
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    let inners = args
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            GenericArgument::Type(inner) => type_cluster_fingerprint(inner),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if !inners.is_empty() {
                        part.push('<');
                        part.push_str(&inners.join(","));
                        part.push('>');
                    }
                }
                parts.push(part);
            }
            Some(parts.join("::"))
        }
        Type::Tuple(tuple) => Some(format!(
            "({})",
            tuple
                .elems
                .iter()
                .filter_map(type_cluster_fingerprint)
                .collect::<Vec<_>>()
                .join(",")
        )),
        Type::Slice(slice) => Some(format!("[{}]", type_cluster_fingerprint(&slice.elem)?)),
        _ => borrowed_or_owned_type_leaf_ident(ty),
    }
}

fn repeated_simplified_type_names(typed_inputs: &[(Option<String>, &Type)]) -> Vec<String> {
    let mut counts = BTreeMap::<String, usize>::new();

    for (_, ty) in typed_inputs {
        let Some(type_name) = borrowed_or_owned_type_leaf_ident(ty) else {
            continue;
        };
        *counts.entry(type_name).or_default() += 1;
    }

    counts
        .into_iter()
        .filter_map(|(type_name, count)| (count > 1).then_some(type_name))
        .collect()
}

fn is_standalone_builder_function_name(name: &str) -> bool {
    matches!(
        name,
        _
            if name.starts_with("with_")
                || name.starts_with("set_")
                || name.starts_with("enable_")
                || name.starts_with("disable_")
                || name.starts_with("apply_")
    )
}

fn standalone_builder_subject_type(item_fn: &ItemFn) -> Option<String> {
    let typed_inputs = typed_inputs(&item_fn.sig.inputs);
    if typed_inputs.len() < 2 {
        return None;
    }

    let subject_type = borrowed_or_owned_type_leaf_ident(typed_inputs.first()?.1)?;
    let returned_type = output_type_leaf_ident(&item_fn.sig.output)?;
    (subject_type == returned_type).then_some(subject_type)
}

fn looks_like_protocol_decision_bool(item_name: &str, bool_name: &str) -> bool {
    let item_tokens = name_tokens(item_name);
    let bool_tokens = name_tokens(bool_name);

    if looks_like_runtime_toggle_name(bool_name) {
        return false;
    }

    let decision_tokens = [
        "approved",
        "rejected",
        "supported",
        "accepted",
        "denied",
        "reviewed",
        "known",
        "allowed",
        "bound",
    ];
    let domain_tokens = [
        "vendor",
        "workflow",
        "review",
        "validate",
        "gate",
        "transition",
        "approval",
        "support",
        "protocol",
        "machine",
        "policy",
        "scope",
        "release",
        "contract",
        "audit",
        "bind",
    ];

    bool_tokens
        .iter()
        .any(|token| decision_tokens.contains(&token.as_str()))
        && (bool_tokens
            .iter()
            .any(|token| domain_tokens.contains(&token.as_str()))
            || item_tokens
                .iter()
                .any(|token| domain_tokens.contains(&token.as_str())))
}

fn block_single_expr(block: &syn::Block) -> Option<&Expr> {
    if block.stmts.len() != 1 {
        return None;
    }

    match &block.stmts[0] {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
}

fn expr_simple_path_ident(expr: &Expr) -> Option<String> {
    let Expr::Path(expr_path) = expr else {
        return None;
    };
    if expr_path.path.segments.len() != 1 {
        return None;
    }

    expr_path.path.get_ident().map(unraw_ident)
}

fn expr_path_leaf_ident(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(expr_path) => expr_path
            .path
            .segments
            .last()
            .map(|segment| unraw_ident(&segment.ident)),
        Expr::Group(group) => expr_path_leaf_ident(&group.expr),
        Expr::Paren(paren) => expr_path_leaf_ident(&paren.expr),
        Expr::Reference(reference) => expr_path_leaf_ident(&reference.expr),
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Deref(_)) => {
            expr_path_leaf_ident(&unary.expr)
        }
        _ => None,
    }
}

fn body_is_match_on_self(block: &syn::Block) -> bool {
    let Some(expr) = block_single_expr(block) else {
        return false;
    };

    let Expr::Match(expr_match) = expr else {
        return false;
    };

    expr_is_self_receiver(expr_match.expr.as_ref())
}

fn expr_is_self_receiver(expr: &Expr) -> bool {
    match expr {
        Expr::Path(expr_path) => path_is_self(&expr_path.path),
        Expr::Unary(expr_unary) => {
            matches!(expr_unary.op, syn::UnOp::Deref(_))
                && expr_is_self_receiver(expr_unary.expr.as_ref())
        }
        _ => false,
    }
}

fn path_is_self(path: &syn::Path) -> bool {
    path.segments.len() == 1 && path.segments[0].ident == "self"
}

fn display_impl_is_literal_string_match(method: &ImplItemFn) -> bool {
    if method.sig.inputs.len() != 2 {
        return false;
    }
    if !matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_))) {
        return false;
    }

    let Some(expr) = block_single_expr(&method.block) else {
        return false;
    };
    let Expr::Match(expr_match) = expr else {
        return false;
    };
    if !expr_is_self_receiver(expr_match.expr.as_ref()) {
        return false;
    }

    expr_match
        .arms
        .iter()
        .all(|arm| expr_is_literal_write_str(arm.body.as_ref()))
}

fn expr_is_literal_write_str(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(method_call) => {
            method_call.method == "write_str"
                && method_call.args.len() == 1
                && method_call.args.first().is_some_and(expr_is_string_literal)
        }
        Expr::Block(expr_block) => {
            block_single_expr(&expr_block.block).is_some_and(expr_is_literal_write_str)
        }
        _ => false,
    }
}

fn expr_is_string_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Lit(syn::ExprLit {
            lit: Lit::Str(_),
            ..
        })
    )
}

fn trait_first_generic_type_leaf_ident(trait_path: &syn::Path) -> Option<String> {
    let segment = trait_path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let first = args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })?;

    borrowed_or_owned_type_leaf_ident(first)
}

fn enum_has_strum_serialize_all(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("strum"))
        .any(|attr| {
            let mut found = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("serialize_all") {
                    found = true;
                }
                if meta.input.peek(syn::Token![=]) {
                    let _ = meta.value()?.parse::<LitStr>()?;
                }
                Ok(())
            });
            found
        })
}

fn variant_single_strum_surface(variant: &syn::Variant) -> Option<String> {
    let mut values = Vec::new();

    for attr in &variant.attrs {
        if !attr.path().is_ident("strum") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("serialize") || meta.path.is_ident("to_string") {
                values.push(meta.value()?.parse::<LitStr>()?.value());
            } else if meta.input.peek(syn::Token![=]) {
                let _ = meta.value()?.parse::<LitStr>()?;
            }
            Ok(())
        })
        .ok()?;
    }

    (values.len() == 1).then(|| values.into_iter().next().expect("len checked"))
}

fn render_strum_case(segments: &[String], case: StrumCase) -> String {
    let lowered = segments
        .iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();

    match case {
        StrumCase::KebabCase => lowered.join("-"),
        StrumCase::SnakeCase => lowered.join("_"),
        StrumCase::ScreamingSnakeCase => lowered.join("_").to_ascii_uppercase(),
        StrumCase::Lowercase => lowered.join(""),
    }
}

fn strum_case_label(case: StrumCase) -> &'static str {
    match case {
        StrumCase::KebabCase => "kebab-case",
        StrumCase::SnakeCase => "snake_case",
        StrumCase::ScreamingSnakeCase => "SCREAMING_SNAKE_CASE",
        StrumCase::Lowercase => "lowercase",
    }
}

fn expr_is_direct_from_forward(expr: &Expr, target_type: &str) -> bool {
    match expr {
        Expr::Call(expr_call) => {
            expr_path_ends_with(expr_call.func.as_ref(), &[target_type, "from"])
                && expr_call.args.len() == 1
                && expr_call.args.first().is_some_and(expr_is_self_or_clone)
        }
        Expr::MethodCall(method_call) => {
            method_call.method == "into"
                && method_call.args.is_empty()
                && expr_is_self_or_clone(method_call.receiver.as_ref())
        }
        Expr::Block(expr_block) => block_single_expr(&expr_block.block)
            .is_some_and(|inner| expr_is_direct_from_forward(inner, target_type)),
        _ => false,
    }
}

fn expr_is_self_or_clone(expr: &Expr) -> bool {
    match expr {
        Expr::Path(expr_path) => path_is_self(&expr_path.path),
        Expr::MethodCall(method_call) => {
            method_call.method == "clone"
                && method_call.args.is_empty()
                && expr_is_self_receiver(method_call.receiver.as_ref())
        }
        Expr::Unary(expr_unary) => {
            matches!(expr_unary.op, syn::UnOp::Deref(_))
                && expr_is_self_receiver(expr_unary.expr.as_ref())
        }
        _ => false,
    }
}

fn expr_path_ends_with(expr: &Expr, segments: &[&str]) -> bool {
    let Expr::Path(expr_path) = expr else {
        return false;
    };
    if expr_path.path.segments.len() < segments.len() {
        return false;
    }

    expr_path
        .path
        .segments
        .iter()
        .rev()
        .zip(segments.iter().rev())
        .all(|(actual, expected)| actual.ident == *expected)
}

fn is_explicit_from_wrapper_name(helper_name: &str, output: &ReturnType) -> bool {
    let Some(target_type) = output_type_leaf_ident(output) else {
        return false;
    };
    let target_name = render_segments(&split_segments(&target_type), NameStyle::Snake);

    helper_name == format!("to_{target_name}") || helper_name == format!("into_{target_name}")
}

fn is_obvious_scalar_type_name(type_name: &str) -> bool {
    matches!(
        type_name,
        "bool"
            | "char"
            | "str"
            | "String"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

fn type_is_result_string_error(ty: &Type) -> bool {
    result_error_type(ty).is_some_and(type_is_string_surface)
}

fn type_is_anyhow_result_or_error(ty: &Type) -> bool {
    type_is_anyhow_error_type(ty) || result_error_type(ty).is_some_and(type_is_anyhow_error_type)
}

fn result_error_type(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    args.args
        .iter()
        .filter_map(|arg| match arg {
            GenericArgument::Type(inner) => Some(inner),
            _ => None,
        })
        .nth(1)
}

fn type_is_anyhow_error_type(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .iter()
        .any(|segment| segment.ident == "anyhow")
        && type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| matches!(segment.ident.to_string().as_str(), "Error" | "Result"))
}

fn looks_like_error_type_name(name: &str) -> bool {
    name == "Error" || name.ends_with("Error")
}

fn flag_const_name(name: &str) -> Option<String> {
    (name.starts_with("FLAG_")
        || name.ends_with("_FLAG")
        || name.contains("_FLAG_")
        || name.ends_with("_FLAGS"))
    .then(|| name.to_string())
}

fn item_name_leaf(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn name_tokens(name: &str) -> Vec<String> {
    split_segments(item_name_leaf(name))
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect()
}

fn name_matches_configured_token_family(name: &str, configured: &BTreeSet<String>) -> bool {
    name_tokens(name)
        .iter()
        .any(|token| configured.contains(token))
}

fn looks_like_error_surface_type_name(name: &str) -> bool {
    let tokens = name_tokens(name);
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "error"
                | "errors"
                | "failure"
                | "failures"
                | "rejection"
                | "rejections"
                | "reject"
                | "rejected"
                | "invalid"
                | "denied"
                | "forbidden"
                | "unauthorized"
        )
    })
}

fn looks_like_error_field_name(type_name: &str, field_name: &str) -> bool {
    let tokens = name_tokens(field_name);
    let has_strong_error_token = tokens
        .iter()
        .any(|token| matches!(token.as_str(), "error" | "errors" | "message"));
    if has_strong_error_token {
        return true;
    }

    let has_soft_error_token = tokens
        .iter()
        .any(|token| matches!(token.as_str(), "reason" | "detail"));
    has_soft_error_token && looks_like_error_surface_type_name(type_name)
}

fn looks_like_semantic_string_scalar_name(name: &str, settings: &NamespaceSettings) -> bool {
    let tokens = name_tokens(name);
    name_matches_configured_token_family(name, &settings.semantic_string_scalars)
        && !tokens.iter().any(|token| token == "id")
}

fn looks_like_semantic_numeric_scalar_name(name: &str, settings: &NamespaceSettings) -> bool {
    name_matches_configured_token_family(name, &settings.semantic_numeric_scalars)
}

fn looks_like_key_value_bag_name(name: &str, settings: &NamespaceSettings) -> bool {
    name_matches_configured_token_family(name, &settings.key_value_bag_names)
}

fn looks_like_runtime_toggle_name(name: &str) -> bool {
    let tokens = name_tokens(name);
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "verbose"
                | "pretty"
                | "recursive"
                | "follow"
                | "force"
                | "color"
                | "dry"
                | "strict"
                | "debug"
                | "compact"
                | "quiet"
        )
    })
}

fn looks_like_render_or_format_helper(name: &str) -> bool {
    matches!(
        name_tokens(name).first().map(String::as_str),
        Some("render" | "format" | "display" | "view" | "as" | "to")
    )
}

fn looks_like_flag_bits_name(name: &str) -> bool {
    let tokens = name_tokens(name);
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "flag" | "flags" | "permission" | "permissions" | "mask" | "bits"
        )
    })
}

fn looks_like_protocol_integer_name(name: &str) -> bool {
    let tokens = name_tokens(name);
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "status" | "kind" | "mode" | "phase" | "step" | "state" | "variant"
        )
    })
}

fn looks_like_id_name(name: &str) -> bool {
    let tokens = name_tokens(name);
    name == "id"
        || name.ends_with("_id")
        || name.ends_with("Id")
        || tokens.iter().any(|token| token == "id")
}

fn string_field_signal(field: &syn::Field) -> Option<StringFieldSignal> {
    let name = field.ident.as_ref().map(unraw_ident)?;
    if !type_is_string_field(&field.ty) {
        return None;
    }

    let tokens = split_segments(&name)
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let strong_metadata_tokens = [
        "state",
        "machine",
        "protocol",
        "artifact",
        "transition",
        "gate",
        "typestate",
        "step",
        "phase",
    ];
    let descriptor_tokens = [
        "kind", "label", "path", "code", "name", "status", "mode", "variant", "next", "current",
        "target",
    ];
    let has_strong_metadata_token = tokens
        .iter()
        .any(|token| strong_metadata_tokens.contains(&token.as_str()));
    let descriptor_hits = tokens
        .iter()
        .filter(|token| descriptor_tokens.contains(&token.as_str()))
        .count();

    Some(StringFieldSignal {
        name,
        scaffold_like: has_strong_metadata_token || descriptor_hits >= 2,
        has_strong_metadata_token,
    })
}

fn type_is_string_field(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => type_is_string_field(&reference.elem),
        Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return false;
            };
            match segment.ident.to_string().as_str() {
                "String" => true,
                "Option" => {
                    let PathArguments::AngleBracketed(args) = &segment.arguments else {
                        return false;
                    };
                    args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => type_is_string_field(inner),
                        _ => false,
                    })
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn type_is_string_collection_surface(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => type_is_string_collection_surface(&reference.elem),
        Type::Slice(slice) => type_is_string_collection_member(&slice.elem),
        Type::Array(array) => type_is_string_collection_member(&array.elem),
        Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return false;
            };
            match segment.ident.to_string().as_str() {
                "Vec" | "HashSet" | "BTreeSet" => {
                    let PathArguments::AngleBracketed(args) = &segment.arguments else {
                        return false;
                    };
                    args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => type_is_string_collection_member(inner),
                        _ => false,
                    })
                }
                "HashMap" | "BTreeMap" => {
                    let PathArguments::AngleBracketed(args) = &segment.arguments else {
                        return false;
                    };
                    args.args.iter().next().is_some_and(|arg| match arg {
                        GenericArgument::Type(inner) => type_is_string_collection_member(inner),
                        _ => false,
                    })
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn type_is_string_key_value_bag_surface(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => type_is_string_key_value_bag_surface(&reference.elem),
        Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return false;
            };
            match segment.ident.to_string().as_str() {
                "HashMap" | "BTreeMap" => {
                    let PathArguments::AngleBracketed(args) = &segment.arguments else {
                        return false;
                    };
                    let mut types = args.args.iter().filter_map(|arg| match arg {
                        GenericArgument::Type(inner) => Some(inner),
                        _ => None,
                    });
                    let Some(key) = types.next() else {
                        return false;
                    };
                    let Some(value) = types.next() else {
                        return false;
                    };
                    type_is_string_collection_member(key) && type_is_string_collection_member(value)
                }
                "Vec" => {
                    let PathArguments::AngleBracketed(args) = &segment.arguments else {
                        return false;
                    };
                    args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => type_is_two_string_tuple(inner),
                        _ => false,
                    })
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn type_is_two_string_tuple(ty: &Type) -> bool {
    let Type::Tuple(tuple) = ty else {
        return false;
    };
    if tuple.elems.len() != 2 {
        return false;
    }

    tuple.elems.iter().all(type_is_string_collection_member)
}

fn type_is_string_collection_member(ty: &Type) -> bool {
    type_is_string_surface(ty) || type_is_string_field(ty)
}

fn collection_name_suggests_protocol_model(name: &str) -> bool {
    let tokens = split_segments(name)
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let protocol_tokens = [
        "state",
        "states",
        "machine",
        "machines",
        "protocol",
        "protocols",
        "artifact",
        "artifacts",
        "transition",
        "transitions",
        "gate",
        "gates",
        "step",
        "steps",
        "phase",
        "phases",
    ];
    let collection_context = ["legal", "allowed", "known", "supported", "available"];

    tokens
        .iter()
        .any(|token| protocol_tokens.contains(&token.as_str()))
        && (tokens
            .iter()
            .any(|token| collection_context.contains(&token.as_str()))
            || tokens.iter().any(|token| token.ends_with('s')))
}

fn looks_like_protocol_descriptor_name(name: &str) -> bool {
    let tokens = split_segments(name)
        .into_iter()
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let strong_tokens = [
        "state",
        "status",
        "mode",
        "phase",
        "step",
        "gate",
        "transition",
        "artifact",
        "machine",
        "protocol",
        "kind",
        "variant",
        "path",
    ];

    tokens
        .iter()
        .any(|token| strong_tokens.contains(&token.as_str()))
}

fn analyze_candidate_semantic_modules(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    scope_flags: ScopeFlags,
    public_bindings: &BTreeSet<String>,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut suppressed_child_module_exports = BTreeSet::new();
    if !scope_flags.path_is_public
        || !scope_flags.allow_candidate_semantic_modules
        || module_path_is_synthetic_scaffolding(module_path)
    {
        return suppressed_child_module_exports;
    }

    let child_modules = semantic_child_module_bindings(path, items, module_path, settings);
    let public_leaves = collect_scope_public_leaf_bindings(items);
    let mut observation_gap = semantic_module_inference_gap_for_scope_items(items);
    merge_semantic_inference_gap(&mut observation_gap, child_modules.observation_gap);
    if let Some(gap) = observation_gap
        && scope_has_candidate_semantic_module_inputs(
            items,
            &public_leaves,
            public_bindings,
            scope_flags.path_is_public,
            settings,
        )
    {
        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(gap.line),
            "api_candidate_semantic_module_unsupported_construct",
            format!(
                "skipped semantic module family inference in this scope because source-level analysis saw {}; `api_candidate_semantic_module` only runs on direct parsed items without cfg pruning or macro expansion",
                render_semantic_inference_constructs(&gap.constructs),
            ),
        ));
        return suppressed_child_module_exports;
    }
    let child_modules = child_modules.bindings;
    let mut families = BTreeMap::<String, Vec<(usize, String)>>::new();

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
            || is_weak_semantic_head(&module_candidate)
        {
            continue;
        }

        families
            .entry(head)
            .or_default()
            .push((binding.line, binding.binding_name.clone()));
    }

    for (_head, members) in families {
        if members.len() < 3 {
            continue;
        }

        let Some(choice) =
            choose_public_shared_head_semantic_family(module_path, &child_modules, &members, 3)
        else {
            continue;
        };

        let line = choice
            .members
            .iter()
            .map(|(line, _)| *line)
            .min()
            .expect("family has at least one member");
        let head = choice.shared_head_segments.concat();
        let module_candidate = render_segments(&choice.shared_head_segments, NameStyle::Snake);
        if semantic_module_candidate_already_emitted(diagnostics, path, &module_candidate) {
            continue;
        }
        let original_members = choice
            .members
            .iter()
            .map(|(_, binding_name)| format!("`{binding_name}`"))
            .collect::<Vec<_>>()
            .join(", ");

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_candidate_semantic_module",
            match choice.suggestion.shared_tail {
                Some(tail) => format!(
                    "public siblings {original_members} share the `{head}` head and `{tail}` tail; consider a semantic `{}` surface",
                    choice.suggestion.surface,
                ),
                None => format!(
                    "public siblings {original_members} share the `{head}` head; consider a semantic `{}` surface",
                    choice.suggestion.surface,
                ),
            },
        ));
    }

    for tail in public_bindings {
        if !settings.generic_nouns.contains(tail) {
            continue;
        }

        let module_candidate = tail.to_ascii_lowercase();
        if settings.weak_modules.contains(&module_candidate)
            || settings.catch_all_modules.contains(&module_candidate)
            || settings.organizational_modules.contains(&module_candidate)
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
        if semantic_module_candidate_already_emitted(diagnostics, path, &module_candidate) {
            continue;
        }
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.original_member))
            .collect::<Vec<_>>()
            .join(", ");
        let inferred_members = members
            .iter()
            .map(|member| member.suggested_leaf.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let Some(suggested_members) = merged_semantic_module_suggested_members(
            &child_modules,
            &module_candidate,
            inferred_members,
        ) else {
            continue;
        };
        let suggested_members = suggested_members.join(", ");

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_candidate_semantic_module",
            format!(
                "public siblings {original_members} share the generic `{tail}` tail; consider a semantic `{module_candidate}::{{{suggested_members}}}` surface"
            ),
        ));

        for member in members {
            if let Some(module_name) = member.child_module_name {
                suppressed_child_module_exports.insert(normalize_segment(&module_name));
            }
        }
    }

    analyze_public_candidate_module_families(path, items, scope_flags, settings, diagnostics);

    suppressed_child_module_exports
}

fn analyze_public_candidate_module_families(
    path: &Path,
    items: &[Item],
    scope_flags: ScopeFlags,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope_members =
        collect_scope_public_module_bindings(items, scope_flags.path_is_public, settings);
    if scope_members.len() < 3 {
        return;
    }

    let minimum_family_members = if scope_has_compound_module_family_pressure(&scope_members) {
        2
    } else {
        3
    };

    let mut head_families = BTreeMap::<String, Vec<ScopeModuleBinding>>::new();
    for member in &scope_members {
        let segments = split_segments(&member.module_name);
        if segments.len() < 2 {
            continue;
        }

        let head = normalize_segment(&segments[0]);
        if is_weak_semantic_head(&head) {
            continue;
        }
        head_families.entry(head).or_default().push(member.clone());
    }

    for members in head_families.into_values() {
        let member_segments = members
            .iter()
            .map(|member| split_segments(&member.module_name))
            .collect::<Vec<_>>();
        let Some(shared_head_segments) =
            candidate_semantic_module_shared_head_segments(&member_segments)
        else {
            continue;
        };
        let minimum_head_family_members = minimum_family_members_for_module_head(
            &scope_members,
            &shared_head_segments,
            minimum_family_members,
        );
        if members.len() < minimum_head_family_members {
            continue;
        }

        let module_candidate = render_segments(&shared_head_segments, NameStyle::Snake);
        let Some(surface) = internal_module_family_surface_for_head(
            &members,
            &shared_head_segments,
            minimum_head_family_members,
        ) else {
            continue;
        };
        if semantic_module_candidate_already_emitted(diagnostics, path, &module_candidate) {
            continue;
        }

        let line = members
            .iter()
            .map(|member| member.line)
            .min()
            .expect("family has members");
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.module_name))
            .collect::<Vec<_>>()
            .join(", ");
        let head = shared_head_segments.concat();

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_candidate_semantic_module",
            format!(
                "public sibling modules {original_members} share the `{head}` head; consider a semantic `{surface}` surface"
            ),
        ));
    }

    let mut tail_families = BTreeMap::<String, Vec<ScopeModuleBinding>>::new();
    for member in &scope_members {
        let segments = split_segments(&member.module_name);
        if segments.len() < 2 {
            continue;
        }
        let Some(tail) = segments.last().map(|segment| normalize_segment(segment)) else {
            continue;
        };
        tail_families.entry(tail).or_default().push(member.clone());
    }

    for (tail, members) in tail_families {
        if members.len() < minimum_family_members
            || settings.weak_modules.contains(&tail)
            || settings.catch_all_modules.contains(&tail)
            || settings.organizational_modules.contains(&tail)
        {
            continue;
        }

        let suggested_members = members
            .iter()
            .filter_map(|member| {
                let segments = split_segments(&member.module_name);
                let shorter_segments = &segments[..segments.len() - 1];
                if shorter_segments.is_empty() {
                    return None;
                }
                let shorter_leaf = render_segments(shorter_segments, NameStyle::Snake);
                (!candidate_semantic_module_shorter_leaf_is_too_generic(&shorter_leaf))
                    .then_some(shorter_leaf)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if suggested_members.len() < minimum_family_members {
            continue;
        }

        let line = members
            .iter()
            .map(|member| member.line)
            .min()
            .expect("family has members");
        if semantic_module_candidate_already_emitted(diagnostics, path, &tail) {
            continue;
        }
        let surface = format!("{tail}::{{{}}}", suggested_members.join(", "));
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.module_name))
            .collect::<Vec<_>>()
            .join(", ");
        let tail_label = render_segments(&split_segments(&tail), NameStyle::Pascal);

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "api_candidate_semantic_module",
            format!(
                "public sibling modules {original_members} share the `{tail_label}` tail; consider a semantic `{surface}` surface"
            ),
        ));
    }
}

fn analyze_internal_candidate_module_families(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    scope_flags: ScopeFlags,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if module_path_contains_internal_namespace(module_path)
        || module_path_is_synthetic_scaffolding(module_path)
    {
        return;
    }

    let scope_members =
        collect_scope_internal_module_bindings(items, scope_flags.path_is_public, settings);
    if scope_members.len() < 3 {
        return;
    }

    let minimum_family_members = if scope_has_compound_module_family_pressure(&scope_members) {
        2
    } else {
        3
    };

    let mut head_families = BTreeMap::<String, Vec<ScopeModuleBinding>>::new();
    for member in &scope_members {
        let segments = split_segments(&member.module_name);
        if segments.len() < 2 {
            continue;
        }

        let head = normalize_segment(&segments[0]);
        if is_weak_semantic_head(&head) {
            continue;
        }
        head_families.entry(head).or_default().push(member.clone());
    }

    for members in head_families.into_values() {
        let member_segments = members
            .iter()
            .map(|member| split_segments(&member.module_name))
            .collect::<Vec<_>>();
        let Some(shared_head_segments) =
            candidate_semantic_module_shared_head_segments(&member_segments)
        else {
            continue;
        };
        let minimum_head_family_members = minimum_family_members_for_module_head(
            &scope_members,
            &shared_head_segments,
            minimum_family_members,
        );
        if members.len() < minimum_head_family_members {
            continue;
        }

        let Some(surface) = internal_module_family_surface_for_head(
            &members,
            &shared_head_segments,
            minimum_head_family_members,
        ) else {
            continue;
        };
        if internal_candidate_surface_already_emitted(diagnostics, path, &surface) {
            continue;
        }
        let line = members
            .iter()
            .map(|member| member.line)
            .min()
            .expect("family has members");
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.module_name))
            .collect::<Vec<_>>()
            .join(", ");
        let head = shared_head_segments.concat();

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "internal_candidate_semantic_module",
            format!(
                "sibling modules {original_members} share the `{head}` head; consider a semantic `{surface}` surface"
            ),
        ));
    }

    let mut tail_families = BTreeMap::<String, Vec<ScopeModuleBinding>>::new();
    for member in &scope_members {
        let segments = split_segments(&member.module_name);
        if segments.len() < 2 {
            continue;
        }
        let Some(tail) = segments.last().map(|segment| normalize_segment(segment)) else {
            continue;
        };
        tail_families.entry(tail).or_default().push(member.clone());
    }

    for (tail, members) in tail_families {
        if members.len() < minimum_family_members
            || settings.weak_modules.contains(&tail)
            || settings.catch_all_modules.contains(&tail)
            || settings.organizational_modules.contains(&tail)
        {
            continue;
        }

        let suggested_members = members
            .iter()
            .filter_map(|member| {
                let segments = split_segments(&member.module_name);
                let shorter_segments = &segments[..segments.len() - 1];
                if shorter_segments.is_empty() {
                    return None;
                }
                let shorter_leaf = render_segments(shorter_segments, NameStyle::Snake);
                (!candidate_semantic_module_shorter_leaf_is_too_generic(&shorter_leaf))
                    .then_some(shorter_leaf)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if suggested_members.len() < minimum_family_members {
            continue;
        }

        let line = members
            .iter()
            .map(|member| member.line)
            .min()
            .expect("family has members");
        let surface = format!("{tail}::{{{}}}", suggested_members.join(", "));
        if internal_candidate_surface_already_emitted(diagnostics, path, &surface) {
            continue;
        }
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.module_name))
            .collect::<Vec<_>>()
            .join(", ");
        let tail_label = render_segments(&split_segments(&tail), NameStyle::Pascal);

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "internal_candidate_semantic_module",
            format!(
                "sibling modules {original_members} share the `{tail_label}` tail; consider a semantic `{surface}` surface",
            ),
        ));
    }
}

fn analyze_internal_candidate_item_families(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    scope_flags: ScopeFlags,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if module_path_contains_internal_namespace(module_path)
        || module_path_is_synthetic_scaffolding(module_path)
    {
        return;
    }

    let members = collect_scope_internal_item_bindings(items, scope_flags.path_is_public);
    if members.len() < 3 {
        return;
    }

    let mut head_families = BTreeMap::<String, Vec<ScopeItemBinding>>::new();
    for member in &members {
        if !matches!(detect_name_style(&member.binding_name), NameStyle::Pascal) {
            continue;
        }

        let segments = split_segments(&member.binding_name);
        if segments.len() < 2 {
            continue;
        }

        let head = normalize_segment(&segments[0]);
        if settings.weak_modules.contains(&head)
            || settings.catch_all_modules.contains(&head)
            || settings.organizational_modules.contains(&head)
            || is_weak_semantic_head(&head)
        {
            continue;
        }

        head_families.entry(head).or_default().push(member.clone());
    }

    for members in head_families.into_values() {
        if members.len() < 3 {
            continue;
        }

        let member_segments = members
            .iter()
            .map(|member| split_segments(&member.binding_name))
            .collect::<Vec<_>>();
        let Some(shared_head_segments) =
            candidate_semantic_module_shared_head_segments(&member_segments)
        else {
            continue;
        };

        let module_candidate = render_segments(&shared_head_segments, NameStyle::Snake);
        let member_entries = members
            .iter()
            .map(|member| (member.line, member.binding_name.clone()))
            .collect::<Vec<_>>();
        if internal_item_family_has_bare_head_tail_member(&member_entries, &shared_head_segments) {
            continue;
        }
        let Some(suggestion) = shared_head_semantic_module_suggestion(
            &BTreeMap::new(),
            &module_candidate,
            &shared_head_segments,
            &member_entries,
        ) else {
            continue;
        };
        let Some(tail) = suggestion.shared_tail.as_deref() else {
            continue;
        };
        if internal_candidate_surface_already_emitted(diagnostics, path, &suggestion.surface) {
            continue;
        }

        let line = members
            .iter()
            .map(|member| member.line)
            .min()
            .expect("family has members");
        let original_members = members
            .iter()
            .map(|member| format!("`{}`", member.binding_name))
            .collect::<Vec<_>>()
            .join(", ");
        let head = shared_head_segments.concat();

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "internal_candidate_semantic_module",
            format!(
                "internal siblings {original_members} share the `{head}` head and `{tail}` tail; consider a semantic `{}` surface",
                suggestion.surface,
            ),
        ));
    }
}

fn internal_item_family_has_bare_head_tail_member(
    members: &[(usize, String)],
    shared_head_segments: &[String],
) -> bool {
    let trimmed_member_segments = members
        .iter()
        .map(|(_, binding_name)| split_segments(binding_name))
        .map(|segments| segments[shared_head_segments.len()..].to_vec())
        .collect::<Vec<_>>();
    let Some(shared_tail_segments) =
        candidate_semantic_module_shared_tail_segments(&trimmed_member_segments)
    else {
        return false;
    };
    let tail_len = shared_tail_segments.len();

    trimmed_member_segments.iter().any(|segments| {
        segments.len() == tail_len
            && normalize_segment(&render_segments(&shared_tail_segments, NameStyle::Pascal))
                == normalize_segment(&render_segments(segments, NameStyle::Pascal))
    })
}

fn internal_module_family_surface_for_head(
    members: &[ScopeModuleBinding],
    shared_head_segments: &[String],
    minimum_family_members: usize,
) -> Option<String> {
    let member_segments = members
        .iter()
        .map(|member| split_segments(&member.module_name))
        .collect::<Vec<_>>();
    let trimmed_member_segments = member_segments
        .iter()
        .map(|segments| segments[shared_head_segments.len()..].to_vec())
        .collect::<Vec<_>>();
    let module_candidate = render_segments(shared_head_segments, NameStyle::Snake);

    if let Some(shared_tail_segments) =
        candidate_semantic_module_shared_tail_segments(&trimmed_member_segments)
    {
        let tail_len = shared_tail_segments.len();
        let nested_members = trimmed_member_segments
            .iter()
            .filter_map(|segments| {
                let middle_segments = &segments[..segments.len() - tail_len];
                if middle_segments.is_empty() {
                    return None;
                }
                let leaf = render_segments(middle_segments, NameStyle::Snake);
                (!candidate_semantic_module_shorter_leaf_is_too_generic(&leaf)).then_some(leaf)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if nested_members.len() >= minimum_family_members {
            let tail_module = render_segments(&shared_tail_segments, NameStyle::Snake);
            return Some(format!(
                "{module_candidate}::{tail_module}::{{{}}}",
                nested_members.join(", ")
            ));
        }
    }

    let inferred_members = trimmed_member_segments
        .into_iter()
        .filter(|segments| !segments.is_empty())
        .map(|segments| render_segments(&segments, NameStyle::Snake))
        .filter(|leaf| !candidate_semantic_module_shorter_leaf_is_too_generic(leaf))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    (inferred_members.len() >= minimum_family_members)
        .then(|| format!("{module_candidate}::{{{}}}", inferred_members.join(", ")))
}

fn internal_candidate_surface_already_emitted(
    diagnostics: &[Diagnostic],
    path: &Path,
    surface: &str,
) -> bool {
    let marker = format!("`{surface}`");
    diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_candidate_semantic_module")
            && diag.file.as_deref() == Some(path)
            && diag.message.contains(&marker)
    })
}

fn scope_has_compound_module_family_pressure(members: &[ScopeModuleBinding]) -> bool {
    members
        .iter()
        .filter(|member| split_segments(&member.module_name).len() >= 2)
        .nth(2)
        .is_some()
}

fn minimum_family_members_for_module_head(
    scope_members: &[ScopeModuleBinding],
    shared_head_segments: &[String],
    default_minimum_family_members: usize,
) -> usize {
    if default_minimum_family_members <= 2
        || !scope_has_bare_head_module(scope_members, shared_head_segments)
    {
        return default_minimum_family_members;
    }

    2
}

fn scope_has_bare_head_module(
    scope_members: &[ScopeModuleBinding],
    shared_head_segments: &[String],
) -> bool {
    let bare_head_module = render_segments(shared_head_segments, NameStyle::Snake);
    scope_members.iter().any(|member| {
        split_segments(&member.module_name).len() == shared_head_segments.len()
            && normalize_segment(&member.module_name) == normalize_segment(&bare_head_module)
    })
}

fn semantic_module_candidate_already_emitted(
    diagnostics: &[Diagnostic],
    path: &Path,
    module_candidate: &str,
) -> bool {
    let marker = format!("`{module_candidate}::");
    diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.file.as_deref() == Some(path)
            && diag.message.contains(&marker)
    })
}

fn merged_semantic_module_suggested_members(
    child_modules: &BTreeMap<String, BTreeSet<String>>,
    module_candidate: &str,
    inferred_members: Vec<String>,
) -> Option<Vec<String>> {
    let existing_members = semantic_child_module_members(child_modules, module_candidate);
    let mut merged_members = BTreeMap::<String, String>::new();

    if let Some(existing_members) = existing_members {
        for member in existing_members {
            merged_members.insert(normalize_segment(member), member.clone());
        }
    }

    let mut adds_new_member = false;
    for member in inferred_members {
        let normalized = normalize_segment(&member);
        if merged_members.contains_key(&normalized) {
            continue;
        }
        merged_members.insert(normalized, member);
        adds_new_member = true;
    }

    adds_new_member.then(|| merged_members.into_values().collect())
}

fn shared_head_semantic_module_suggestion(
    child_modules: &BTreeMap<String, BTreeSet<String>>,
    module_candidate: &str,
    shared_head_segments: &[String],
    members: &[(usize, String)],
) -> Option<SharedHeadSemanticModuleSuggestion> {
    let member_segments = members
        .iter()
        .map(|(_, binding_name)| split_segments(binding_name))
        .collect::<Vec<_>>();
    let trimmed_member_segments = member_segments
        .iter()
        .map(|segments| segments[shared_head_segments.len()..].to_vec())
        .collect::<Vec<_>>();

    if let Some(shared_tail_segments) =
        candidate_semantic_module_shared_tail_segments(&trimmed_member_segments)
    {
        let tail_len = shared_tail_segments.len();
        let mut root_members = BTreeMap::<String, String>::new();
        let mut nested_members = BTreeMap::<String, String>::new();

        if let Some(existing_members) =
            semantic_child_module_members(child_modules, module_candidate)
        {
            for member in existing_members {
                root_members.insert(normalize_segment(member), member.clone());
            }
        }

        for ((_, binding_name), trimmed_segments) in
            members.iter().zip(trimmed_member_segments.iter())
        {
            let middle_segments = &trimmed_segments[..trimmed_segments.len() - tail_len];
            if middle_segments.is_empty() {
                let leaf = render_segments(&shared_tail_segments, detect_name_style(binding_name));
                root_members.insert(normalize_segment(&leaf), leaf);
                continue;
            }

            let leaf = render_segments(middle_segments, detect_name_style(binding_name));
            nested_members.insert(normalize_segment(&leaf), leaf);
        }

        if !nested_members.is_empty()
            && !nested_members
                .values()
                .all(|leaf| candidate_semantic_module_shorter_leaf_is_too_generic(leaf))
        {
            let tail = render_segments(&shared_tail_segments, NameStyle::Pascal);
            let tail_module = render_segments(&shared_tail_segments, NameStyle::Snake);
            let nested_surface = format!(
                "{tail_module}::{{{}}}",
                nested_members.into_values().collect::<Vec<_>>().join(", ")
            );
            let surface = if root_members.is_empty() {
                format!("{module_candidate}::{nested_surface}")
            } else {
                let mut outer_members = root_members.into_values().collect::<Vec<_>>();
                outer_members.push(nested_surface);
                format!("{module_candidate}::{{{}}}", outer_members.join(", "))
            };

            return Some(SharedHeadSemanticModuleSuggestion {
                shared_tail: Some(tail),
                surface,
            });
        }
    }

    let inferred_members = members
        .iter()
        .zip(member_segments)
        .map(|((_, binding_name), segments)| {
            render_segments(
                &segments[shared_head_segments.len()..],
                detect_name_style(binding_name),
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if inferred_members
        .iter()
        .all(|shorter_leaf| candidate_semantic_module_shorter_leaf_is_too_generic(shorter_leaf))
    {
        return None;
    }
    let suggested_members = merged_semantic_module_suggested_members(
        child_modules,
        module_candidate,
        inferred_members,
    )?;

    Some(SharedHeadSemanticModuleSuggestion {
        shared_tail: None,
        surface: format!("{module_candidate}::{{{}}}", suggested_members.join(", ")),
    })
}

fn choose_public_shared_head_semantic_family(
    module_path: &[String],
    child_modules: &BTreeMap<String, BTreeSet<String>>,
    members: &[(usize, String)],
    minimum_family_members: usize,
) -> Option<SharedHeadSemanticFamilyChoice> {
    let member_segments = members
        .iter()
        .map(|(_, binding_name)| split_segments(binding_name))
        .collect::<Vec<_>>();
    let dominant_compound_minimum =
        dominant_compound_head_minimum_family_members(members.len(), minimum_family_members);
    let mut seen_prefixes = BTreeSet::new();
    let mut best_compound = None::<(usize, usize, SharedHeadSemanticFamilyChoice)>;

    for segments in &member_segments {
        for prefix_len in 2..segments.len() {
            let prefix = segments[..prefix_len].to_vec();
            let prefix_key = render_segments(&prefix, NameStyle::Snake);
            if !seen_prefixes.insert(prefix_key.clone()) {
                continue;
            }

            let candidate_members = members
                .iter()
                .cloned()
                .filter(|(_, binding_name)| {
                    let binding_segments = split_segments(binding_name);
                    binding_segments.len() > prefix_len
                        && segments_start_with_normalized(&binding_segments, &prefix)
                })
                .collect::<Vec<_>>();
            if candidate_members.len() < dominant_compound_minimum
                || shared_head_candidate_redundant_with_parent_module(
                    module_path,
                    &prefix,
                    &candidate_members,
                )
            {
                continue;
            }

            let Some(suggestion) = shared_head_semantic_module_suggestion(
                child_modules,
                &prefix_key,
                &prefix,
                &candidate_members,
            ) else {
                continue;
            };

            let score = (candidate_members.len(), prefix_len);
            let choice = SharedHeadSemanticFamilyChoice {
                shared_head_segments: prefix,
                members: candidate_members,
                suggestion,
            };
            if best_compound
                .as_ref()
                .is_none_or(|best| score > (best.0, best.1))
            {
                best_compound = Some((score.0, score.1, choice));
            }
        }
    }

    if let Some((_, _, choice)) = best_compound {
        return Some(choice);
    }

    let shared_head_segments = candidate_semantic_module_shared_head_segments(&member_segments)?;
    if shared_head_candidate_redundant_with_parent_module(
        module_path,
        &shared_head_segments,
        members,
    ) {
        return None;
    }
    let module_candidate = render_segments(&shared_head_segments, NameStyle::Snake);
    let suggestion = shared_head_semantic_module_suggestion(
        child_modules,
        &module_candidate,
        &shared_head_segments,
        members,
    )?;

    Some(SharedHeadSemanticFamilyChoice {
        shared_head_segments,
        members: members.to_vec(),
        suggestion,
    })
}

fn dominant_compound_head_minimum_family_members(
    family_member_count: usize,
    minimum_family_members: usize,
) -> usize {
    minimum_family_members.max((family_member_count * 2).div_ceil(3))
}

fn shared_head_candidate_redundant_with_parent_module(
    module_path: &[String],
    shared_head_segments: &[String],
    members: &[(usize, String)],
) -> bool {
    let Some(parent_module) = module_path.last() else {
        return false;
    };
    let parent_segments = split_segments(parent_module);
    if parent_segments.is_empty() {
        return false;
    }

    if segments_equal_normalized(&parent_segments, shared_head_segments) {
        return true;
    }
    if parent_segments.len() <= shared_head_segments.len()
        || !segments_start_with_normalized(&parent_segments, shared_head_segments)
    {
        return false;
    }

    let parent_remainder = &parent_segments[shared_head_segments.len()..];
    members.iter().any(|(_, binding_name)| {
        let binding_segments = split_segments(binding_name);
        binding_segments.len() > shared_head_segments.len()
            && segments_start_with_normalized(&binding_segments, shared_head_segments)
            && segments_equal_normalized(
                &binding_segments[shared_head_segments.len()..],
                parent_remainder,
            )
    })
}

fn segments_start_with_normalized(segments: &[String], prefix: &[String]) -> bool {
    segments.len() >= prefix.len()
        && segments
            .iter()
            .zip(prefix.iter())
            .take(prefix.len())
            .all(|(left, right)| normalize_segment(left) == normalize_segment(right))
}

fn segments_equal_normalized(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| normalize_segment(left) == normalize_segment(right))
}

fn semantic_child_module_members<'a>(
    child_modules: &'a BTreeMap<String, BTreeSet<String>>,
    module_candidate: &str,
) -> Option<&'a BTreeSet<String>> {
    child_modules.iter().find_map(|(module_name, bindings)| {
        (normalize_segment(module_name) == normalize_segment(module_candidate)).then_some(bindings)
    })
}

fn module_path_is_synthetic_scaffolding(module_path: &[String]) -> bool {
    module_path
        .iter()
        .any(|segment| module_name_is_synthetic_scaffolding(segment))
}

fn internal_candidate_semantic_module_inference_blocked(
    items: &[Item],
    module_path: &[String],
    scope_flags: ScopeFlags,
    settings: &NamespaceSettings,
) -> bool {
    if module_path_contains_internal_namespace(module_path)
        || module_path_is_synthetic_scaffolding(module_path)
    {
        return false;
    }

    if internal_semantic_module_inference_gap_for_scope_items(items).is_none() {
        return false;
    }
    if !scope_has_internal_candidate_semantic_module_inputs(
        items,
        scope_flags.path_is_public,
        settings,
    ) {
        return false;
    }
    true
}

fn scope_has_internal_candidate_semantic_module_inputs(
    items: &[Item],
    path_is_public: bool,
    settings: &NamespaceSettings,
) -> bool {
    collect_scope_internal_module_bindings(items, path_is_public, settings)
        .iter()
        .filter(|binding| split_segments(&binding.module_name).len() >= 2)
        .nth(1)
        .is_some()
        || collect_scope_internal_item_bindings(items, path_is_public)
            .iter()
            .filter(|binding| {
                matches!(detect_name_style(&binding.binding_name), NameStyle::Pascal)
                    && split_segments(&binding.binding_name).len() >= 2
            })
            .nth(1)
            .is_some()
}

fn module_name_is_synthetic_scaffolding(module_name: &str) -> bool {
    matches!(
        normalize_segment(module_name).as_str(),
        "compile_test" | "compile_tests" | "fixture" | "fixtures"
    )
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
        None,
        module_name,
        None,
        settings,
    )?;
    (!public_bindings.contains(&surface_export.parent_binding)).then_some(surface_export)
}

fn child_module_surface_export_candidate(
    path: &Path,
    module_path: &[String],
    item_mod: Option<&ItemMod>,
    module_name: &str,
    inline_items: Option<&[Item]>,
    settings: &NamespaceSettings,
) -> Option<ChildModuleSurfaceExport> {
    if settings
        .organizational_modules
        .contains(&normalize_segment(module_name))
        || is_internal_module_name(module_name)
        || item_mod.is_some_and(module_is_hidden_or_internal)
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

fn analyze_item_shape(
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
    if !is_internal_shape_candidate_item(item) {
        return;
    }

    if path_is_public && is_item_public {
        analyze_public_leaf(
            path,
            line,
            scope_items,
            module_path,
            &leaf_name,
            None,
            settings,
            diagnostics,
        );
    } else {
        analyze_internal_leaf(path, line, module_path, &leaf_name, settings, diagnostics);
    }
}

fn analyze_public_leaf(
    path: &Path,
    line: usize,
    scope_items: &[Item],
    module_path: &[String],
    leaf_name: &str,
    source_use_leaf: Option<&PublicUseLeaf>,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(preferred_path) =
        semantic_module_surface_candidate(path, scope_items, module_path, leaf_name, settings)
            .filter(|candidate| {
                !scope_has_shorter_sibling_surface_export(
                    scope_items,
                    module_path,
                    source_use_leaf,
                    candidate,
                )
            })
    {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_redundant_leaf_context",
            format!(
                "public API already exposes `{}`; prefer it over `{}`",
                preferred_path.preferred_path,
                render_public_path(module_path, leaf_name),
            ),
        ));
        return;
    }

    let Some(parent_module) = module_path.last() else {
        return;
    };
    let parent_normalized = normalize_segment(parent_module);

    if settings.weak_modules.contains(&parent_normalized)
        && settings.generic_nouns.contains(leaf_name)
    {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_weak_module_generic_leaf",
            format!(
                "`{}` is too generic for weak module `{parent_module}`; keep the domain in the leaf or choose a stronger module",
                render_public_path(module_path, leaf_name),
            ),
        ));
        return;
    }

    if let Some(shorter_leaf) = redundant_category_suffix_leaf(parent_module, leaf_name, settings) {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_redundant_category_suffix",
            format!(
                "`{}` repeats the `{parent_module}` category; prefer `{}`",
                render_public_path(module_path, leaf_name),
                render_preferred_public_path(module_path, &shorter_leaf, settings)
            ),
        ));
        return;
    }

    if settings.weak_modules.contains(&parent_normalized)
        || settings.catch_all_modules.contains(&parent_normalized)
    {
        return;
    }

    if let Some(shorter_leaf) = redundant_leaf_context_candidate(parent_module, leaf_name) {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "api_redundant_leaf_context",
            format!(
                "`{}` repeats the `{parent_module}` context; prefer `{}`",
                render_public_path(module_path, leaf_name),
                render_preferred_public_path(module_path, &shorter_leaf, settings)
            ),
        ));
    }
}

fn analyze_internal_leaf(
    path: &Path,
    line: usize,
    module_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(parent_module) = module_path.last() else {
        return;
    };
    let parent_normalized = normalize_segment(parent_module);

    if settings.weak_modules.contains(&parent_normalized)
        && settings.generic_nouns.contains(leaf_name)
    {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "internal_weak_module_generic_leaf",
            format!(
                "internal item `{}` is too generic for weak module `{parent_module}`; keep the domain in the leaf or choose a stronger module",
                render_public_path(module_path, leaf_name),
            ),
        ));
        return;
    }

    if let Some(shorter_leaf) = redundant_category_suffix_leaf(parent_module, leaf_name, settings) {
        diagnostics.push(Diagnostic::policy(
            Some(path.to_path_buf()),
            Some(line),
            "internal_redundant_category_suffix",
            format!(
                "internal item `{}` repeats the `{parent_module}` category; prefer `{}`",
                render_public_path(module_path, leaf_name),
                render_public_path(module_path, &shorter_leaf),
            ),
        ));
        return;
    }

    if settings.weak_modules.contains(&parent_normalized)
        || settings.catch_all_modules.contains(&parent_normalized)
    {
        return;
    }

    if let Some(shorter_leaf) = redundant_leaf_context_candidate(parent_module, leaf_name) {
        if internal_shorter_leaf_is_too_generic(&shorter_leaf) {
            return;
        }
        if internal_leaf_context_is_human_facing_machinery(module_path, leaf_name) {
            return;
        }

        let path_text = render_public_path(module_path, leaf_name);
        let preferred_path = render_public_path(module_path, &shorter_leaf);
        if internal_leaf_context_is_adapter_policy(parent_module, &shorter_leaf) {
            diagnostics.push(Diagnostic::policy(
                Some(path.to_path_buf()),
                Some(line),
                "internal_adapter_redundant_leaf_context",
                format!(
                    "internal adapter `{path_text}` repeats the `{parent_module}` implementation context; prefer `{preferred_path}`",
                ),
            ));
            return;
        }

        diagnostics.push(Diagnostic::advisory(
            Some(path.to_path_buf()),
            Some(line),
            "internal_redundant_leaf_context",
            format!(
                "internal item `{path_text}` repeats the `{parent_module}` context; prefer `{preferred_path}`",
            ),
        ));
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

fn scope_has_shorter_sibling_surface_export(
    scope_items: &[Item],
    module_path: &[String],
    source_use_leaf: Option<&PublicUseLeaf>,
    candidate: &SemanticModuleSurfaceCandidate,
) -> bool {
    let Some(source_use_leaf) = source_use_leaf else {
        return false;
    };

    let expected_path = {
        let mut path = module_path.to_vec();
        path.push(candidate.module_name.clone());
        path.push(candidate.shorter_leaf.clone());
        path
    };

    scope_items.iter().any(|item| {
        let Item::Use(item_use) = item else {
            return false;
        };
        if !is_public(&item_use.vis) {
            return false;
        }

        let mut leaves = Vec::new();
        flatten_public_use_tree(Vec::new(), &item_use.tree, &mut leaves);
        leaves.into_iter().any(|leaf| {
            leaf.binding_name == candidate.shorter_leaf
                && leaf.binding_name != source_use_leaf.binding_name
                && resolve_local_module_path(module_path, &leaf.full_path)
                    .is_some_and(|resolved| resolved == expected_path)
        })
    })
}

fn semantic_module_surface_candidate(
    path: &Path,
    scope_items: &[Item],
    module_path: &[String],
    leaf_name: &str,
    settings: &NamespaceSettings,
) -> Option<SemanticModuleSurfaceCandidate> {
    let child_module_bindings =
        semantic_child_module_bindings(path, scope_items, module_path, settings);
    if child_module_bindings.bindings.is_empty() {
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

    for (module_name, bindings) in child_module_bindings.bindings {
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

            return Some(SemanticModuleSurfaceCandidate {
                preferred_path: render_public_path_with_module(
                    module_path,
                    &module_name,
                    &shorter_leaf,
                ),
                module_name,
                shorter_leaf,
            });
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

        return Some(SemanticModuleSurfaceCandidate {
            preferred_path: render_public_path_with_module(
                module_path,
                &module_name,
                &shorter_leaf,
            ),
            module_name,
            shorter_leaf,
        });
    }

    None
}

fn semantic_child_module_bindings(
    path: &Path,
    scope_items: &[Item],
    module_path: &[String],
    settings: &NamespaceSettings,
) -> SemanticChildModuleBindings {
    let mut bindings = BTreeMap::new();
    let mut observation_gap = None;

    for item in scope_items {
        let Item::Mod(item_mod) = item else {
            continue;
        };
        if !is_public(&item_mod.vis) {
            continue;
        }
        if module_is_hidden_or_internal(item_mod) {
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
        for construct in child_bindings.observation_gap_constructs {
            note_semantic_inference_construct(
                &mut observation_gap,
                item_mod.span().start().line,
                construct,
            );
        }
        if child_bindings.bindings.is_empty() {
            continue;
        }

        bindings.insert(module_name, child_bindings.bindings);
    }

    SemanticChildModuleBindings {
        bindings,
        observation_gap,
    }
}

fn is_weak_semantic_head(head: &str) -> bool {
    matches!(head, "to" | "has" | "open" | "rolled")
}

fn candidate_semantic_module_shared_head_segments(
    member_segments: &[Vec<String>],
) -> Option<Vec<String>> {
    let first = member_segments.first()?;
    let shared_prefix_len =
        member_segments
            .iter()
            .skip(1)
            .fold(first.len(), |current, segments| {
                current.min(
                    first
                        .iter()
                        .zip(segments.iter())
                        .take_while(|(left, right)| {
                            normalize_segment(left) == normalize_segment(right)
                        })
                        .count(),
                )
            });

    (shared_prefix_len > 0
        && member_segments
            .iter()
            .all(|segments| segments.len() > shared_prefix_len))
    .then(|| first[..shared_prefix_len].to_vec())
}

fn candidate_semantic_module_shared_tail_segments(
    member_segments: &[Vec<String>],
) -> Option<Vec<String>> {
    let first = member_segments.first()?;
    let shared_suffix_len =
        member_segments
            .iter()
            .skip(1)
            .fold(first.len(), |current, segments| {
                current.min(
                    first
                        .iter()
                        .rev()
                        .zip(segments.iter().rev())
                        .take_while(|(left, right)| {
                            normalize_segment(left) == normalize_segment(right)
                        })
                        .count(),
                )
            });

    (shared_suffix_len > 0).then(|| first[first.len() - shared_suffix_len..].to_vec())
}

fn candidate_semantic_module_shorter_leaf_is_too_generic(shorter_leaf: &str) -> bool {
    let weak_tokens = [
        "class",
        "classes",
        "code",
        "codes",
        "fix",
        "fixes",
        "info",
        "infos",
        "kind",
        "kinds",
        "level",
        "levels",
        "selection",
        "selections",
        "state",
        "states",
        "type",
        "types",
    ];
    let tokens = split_segments(shorter_leaf)
        .into_iter()
        .map(|segment| normalize_segment(&segment))
        .collect::<Vec<_>>();

    !tokens.is_empty()
        && tokens
            .iter()
            .all(|token| weak_tokens.contains(&token.as_str()))
}

fn module_is_hidden_or_internal(item_mod: &ItemMod) -> bool {
    is_internal_module_name(&item_mod.ident.to_string())
        || item_mod.attrs.iter().any(attribute_is_doc_hidden)
}

fn module_path_contains_internal_namespace(module_path: &[String]) -> bool {
    module_path
        .iter()
        .any(|segment| is_internal_module_name(segment))
}

fn is_internal_module_name(module_name: &str) -> bool {
    let normalized = normalize_segment(module_name);
    normalized.starts_with("__")
        || matches!(
            normalized.as_str(),
            "internal" | "private" | "detail" | "details"
        )
}

fn attribute_is_doc_hidden(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("doc") {
        return false;
    }

    let mut hidden = false;
    let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("hidden") {
            hidden = true;
        }
        Ok(())
    });
    hidden
}

fn has_builder_attribute(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("builder"))
}

fn public_bindings_for_child_module(
    current_file: &Path,
    module_path: &[String],
    module_name: &str,
    inline_items: Option<&[Item]>,
) -> ChildModulePublicBindings {
    if let Some(items) = inline_items {
        return ChildModulePublicBindings {
            bindings: collect_scope_public_bindings(items),
            observation_gap_constructs: semantic_module_inference_constructs_in_items(items),
        };
    }

    let Some(src_root) = source_root(current_file) else {
        return ChildModulePublicBindings {
            bindings: BTreeSet::new(),
            observation_gap_constructs: BTreeSet::new(),
        };
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
        return ChildModulePublicBindings {
            bindings: collect_scope_public_bindings(&parsed.items),
            observation_gap_constructs: semantic_module_inference_constructs_in_items(
                &parsed.items,
            ),
        };
    }

    ChildModulePublicBindings {
        bindings: BTreeSet::new(),
        observation_gap_constructs: BTreeSet::new(),
    }
}

fn semantic_module_inference_gap_for_scope_items(
    items: &[Item],
) -> Option<SemanticInferenceObservationGap> {
    let mut gap = None;

    for item in items {
        for construct in semantic_module_inference_constructs_for_item(item) {
            note_semantic_inference_construct(&mut gap, item.span().start().line, construct);
        }
    }

    gap
}

fn internal_semantic_module_inference_gap_for_scope_items(
    items: &[Item],
) -> Option<SemanticInferenceObservationGap> {
    let mut gap = None;

    for item in items {
        let mut constructs = BTreeSet::new();
        for attr in item_attrs(item) {
            if attr.path().is_ident("cfg") {
                constructs.insert("#[cfg]".to_string());
            }
            if attr.path().is_ident("cfg_attr") {
                constructs.insert("#[cfg_attr]".to_string());
            }
        }
        if let Item::Macro(item_macro) = item {
            constructs.insert(item_macro_observation_construct(item_macro).to_string());
        }

        for construct in constructs {
            note_semantic_inference_construct(&mut gap, item.span().start().line, construct);
        }
    }

    gap
}

fn semantic_module_inference_constructs_in_items(items: &[Item]) -> BTreeSet<String> {
    let mut constructs = BTreeSet::new();

    for item in items {
        constructs.extend(semantic_module_inference_constructs_for_item(item));
    }

    constructs
}

fn semantic_module_inference_constructs_for_item(item: &Item) -> BTreeSet<String> {
    let mut constructs = BTreeSet::new();

    if item_participates_in_public_surface(item) {
        for attr in item_attrs(item) {
            if attr.path().is_ident("cfg") {
                constructs.insert("#[cfg]".to_string());
            }
            if attr.path().is_ident("cfg_attr") {
                constructs.insert("#[cfg_attr]".to_string());
            }
        }
    }

    if let Item::Macro(item_macro) = item {
        constructs.insert(item_macro_observation_construct(item_macro).to_string());
    }

    constructs
}

fn scope_has_candidate_semantic_module_inputs(
    items: &[Item],
    public_leaves: &[PublicLeafBinding],
    public_bindings: &BTreeSet<String>,
    path_is_public: bool,
    settings: &NamespaceSettings,
) -> bool {
    public_leaves.iter().any(|binding| {
        matches!(detect_name_style(&binding.binding_name), NameStyle::Pascal)
            && split_segments(&binding.binding_name).len() >= 2
    }) || items.iter().any(|item| matches!(item, Item::Macro(_)))
        || scope_has_public_candidate_module_family_inputs(items, path_is_public, settings)
        || (public_bindings
            .iter()
            .any(|binding| settings.generic_nouns.contains(binding))
            && scope_has_public_candidate_child_module(items, settings))
}

fn scope_has_public_candidate_module_family_inputs(
    items: &[Item],
    path_is_public: bool,
    settings: &NamespaceSettings,
) -> bool {
    let scope_members = collect_scope_public_module_bindings(items, path_is_public, settings);
    if scope_members.len() < 2 {
        return false;
    }

    let minimum_family_members = if scope_has_compound_module_family_pressure(&scope_members) {
        2
    } else {
        3
    };

    let mut head_families = BTreeMap::<String, Vec<ScopeModuleBinding>>::new();
    for member in &scope_members {
        let segments = split_segments(&member.module_name);
        if segments.len() < 2 {
            continue;
        }

        let head = normalize_segment(&segments[0]);
        if is_weak_semantic_head(&head) {
            continue;
        }
        head_families.entry(head).or_default().push(member.clone());
    }

    if head_families.into_values().any(|members| {
        let member_segments = members
            .iter()
            .map(|member| split_segments(&member.module_name))
            .collect::<Vec<_>>();
        let Some(shared_head_segments) =
            candidate_semantic_module_shared_head_segments(&member_segments)
        else {
            return false;
        };
        let minimum_head_family_members = minimum_family_members_for_module_head(
            &scope_members,
            &shared_head_segments,
            minimum_family_members,
        );
        members.len() >= minimum_head_family_members
    }) {
        return true;
    }

    let mut tail_families = BTreeMap::<String, Vec<ScopeModuleBinding>>::new();
    for member in &scope_members {
        let segments = split_segments(&member.module_name);
        if segments.len() < 2 {
            continue;
        }
        let Some(tail) = segments.last().map(|segment| normalize_segment(segment)) else {
            continue;
        };
        tail_families.entry(tail).or_default().push(member.clone());
    }

    tail_families.into_iter().any(|(tail, members)| {
        if members.len() < minimum_family_members
            || settings.weak_modules.contains(&tail)
            || settings.catch_all_modules.contains(&tail)
            || settings.organizational_modules.contains(&tail)
        {
            return false;
        }

        let suggested_members = members
            .iter()
            .filter_map(|member| {
                let segments = split_segments(&member.module_name);
                let shorter_segments = &segments[..segments.len() - 1];
                if shorter_segments.is_empty() {
                    return None;
                }
                let shorter_leaf = render_segments(shorter_segments, NameStyle::Snake);
                (!candidate_semantic_module_shorter_leaf_is_too_generic(&shorter_leaf))
                    .then_some(shorter_leaf)
            })
            .collect::<BTreeSet<_>>();

        suggested_members.len() >= minimum_family_members
    })
}

fn scope_has_public_candidate_child_module(items: &[Item], settings: &NamespaceSettings) -> bool {
    items.iter().any(|item| {
        let Item::Mod(item_mod) = item else {
            return false;
        };
        if !is_public(&item_mod.vis) || module_is_hidden_or_internal(item_mod) {
            return false;
        }

        let normalized = normalize_segment(&item_mod.ident.to_string());
        !(settings.weak_modules.contains(&normalized)
            || settings.catch_all_modules.contains(&normalized)
            || settings.organizational_modules.contains(&normalized))
    })
}

fn item_participates_in_public_surface(item: &Item) -> bool {
    match item {
        Item::Const(item_const) => is_public(&item_const.vis),
        Item::Enum(item_enum) => is_public(&item_enum.vis),
        Item::Fn(item_fn) => is_public(&item_fn.vis),
        Item::Mod(item_mod) => is_public(&item_mod.vis),
        Item::Static(item_static) => is_public(&item_static.vis),
        Item::Struct(item_struct) => is_public(&item_struct.vis),
        Item::Trait(item_trait) => is_public(&item_trait.vis),
        Item::TraitAlias(item_trait_alias) => is_public(&item_trait_alias.vis),
        Item::Type(item_type) => is_public(&item_type.vis),
        Item::Union(item_union) => is_public(&item_union.vis),
        Item::Use(item_use) => is_public(&item_use.vis),
        Item::Macro(_) => true,
        _ => false,
    }
}

fn item_attrs(item: &Item) -> &[syn::Attribute] {
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

fn item_macro_observation_construct(item_macro: &ItemMacro) -> &'static str {
    if item_macro.mac.path.is_ident("include") {
        "include!"
    } else if item_macro.mac.path.is_ident("macro_rules") {
        "macro_rules!"
    } else {
        "item macro"
    }
}

fn note_semantic_inference_construct(
    gap: &mut Option<SemanticInferenceObservationGap>,
    line: usize,
    construct: String,
) {
    match gap {
        Some(existing) => {
            existing.line = existing.line.min(line);
            existing.constructs.insert(construct);
        }
        None => {
            let mut constructs = BTreeSet::new();
            constructs.insert(construct);
            *gap = Some(SemanticInferenceObservationGap { line, constructs });
        }
    }
}

fn merge_semantic_inference_gap(
    target: &mut Option<SemanticInferenceObservationGap>,
    gap: Option<SemanticInferenceObservationGap>,
) {
    let Some(gap) = gap else {
        return;
    };

    let line = gap.line;
    for construct in gap.constructs {
        note_semantic_inference_construct(target, line, construct);
    }
}

fn render_semantic_inference_constructs(constructs: &BTreeSet<String>) -> String {
    constructs
        .iter()
        .map(|construct| format!("`{construct}`"))
        .collect::<Vec<_>>()
        .join(", ")
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

fn is_internal_shape_candidate_item(item: &Item) -> bool {
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

fn internal_shorter_leaf_is_too_generic(shorter_leaf: &str) -> bool {
    let weak_tokens = [
        "ext",
        "ctx",
        "config",
        "kind",
        "input",
        "output",
        "items",
        "status",
        "shared",
        "global",
        "part",
        "parts",
        "attrs",
        "attr",
        "param",
        "params",
        "name",
        "names",
        "doc",
        "docs",
        "default",
        "parsing",
        "gen",
        "member",
        "message",
        "state",
        "origin",
        "error",
        "request",
        "response",
        "outcome",
        "fn",
        "fns",
        "impl",
        "raw",
        "skip",
        "internal",
        "signature",
        "for",
        "lookup",
        "module",
        "path",
        "machine",
        "key",
        "analysis",
        "class",
        "level",
        "fix",
        "code",
        "info",
        "selection",
    ];
    let tokens = name_tokens(shorter_leaf);

    !tokens.is_empty()
        && tokens
            .iter()
            .all(|token| weak_tokens.contains(&token.as_str()))
}

fn internal_leaf_context_is_adapter_policy(parent_module: &str, shorter_leaf: &str) -> bool {
    let adapter_context_tokens = [
        "adapter",
        "grpc",
        "http",
        "https",
        "memory",
        "mock",
        "mysql",
        "postgres",
        "postgresql",
        "redis",
        "sqlite",
    ];
    let adapter_role_tokens = [
        "adapter",
        "backend",
        "client",
        "connector",
        "gateway",
        "repository",
        "store",
        "transport",
    ];
    let parent_tokens = name_tokens(parent_module);
    let shorter_tokens = name_tokens(shorter_leaf);

    parent_tokens
        .iter()
        .any(|token| adapter_context_tokens.contains(&token.as_str()))
        && shorter_tokens
            .last()
            .is_some_and(|token| adapter_role_tokens.contains(&token.as_str()))
}

fn namespace_preserving_module_tail_candidate(
    module_name: &str,
    settings: &NamespaceSettings,
) -> Option<(String, String)> {
    let segments = split_segments(module_name);
    if segments.len() < 2 {
        return None;
    }

    for tail_len in (1..segments.len()).rev() {
        let head_segments = &segments[..segments.len() - tail_len];
        let tail_segments = &segments[segments.len() - tail_len..];
        if head_segments.is_empty() {
            continue;
        }

        let tail_module = render_segments(tail_segments, NameStyle::Snake);
        if !settings.namespace_preserving_modules.contains(&tail_module)
            || !namespace_preserving_module_tail_is_actionable(&tail_module)
        {
            continue;
        }

        let head_module = render_segments(head_segments, NameStyle::Snake);
        if settings.weak_modules.contains(&head_module)
            || settings.catch_all_modules.contains(&head_module)
            || settings.organizational_modules.contains(&head_module)
        {
            return None;
        }

        return Some((head_module, tail_module));
    }

    None
}

fn namespace_preserving_module_tail_is_actionable(tail_module: &str) -> bool {
    matches!(
        tail_module,
        "command" | "email" | "http" | "policy" | "query" | "repo" | "store" | "write_back"
    )
}

fn internal_leaf_context_is_human_facing_machinery(
    module_path: &[String],
    leaf_name: &str,
) -> bool {
    let path_tokens = module_path
        .iter()
        .flat_map(|segment| name_tokens(segment))
        .collect::<Vec<_>>();
    if path_tokens
        .iter()
        .any(|token| matches!(token.as_str(), "replay" | "trace"))
    {
        return true;
    }

    let typestate_tokens = [
        "flow",
        "gate",
        "graph",
        "machine",
        "protocol",
        "state",
        "transition",
        "typestate",
    ];
    let leaf_tokens = name_tokens(leaf_name);

    path_tokens
        .iter()
        .any(|token| typestate_tokens.contains(&token.as_str()))
        && leaf_tokens
            .iter()
            .any(|token| typestate_tokens.contains(&token.as_str()))
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
fn module_decl_visibility(file: &Path, segment: &str) -> Option<bool> {
    let src = fs::read_to_string(file).ok()?;
    let parsed = syn::parse_file(&src).ok()?;

    parsed.items.into_iter().find_map(|item| match item {
        Item::Mod(item_mod) if item_mod.ident == segment => Some(is_public(&item_mod.vis)),
        _ => None,
    })
}
