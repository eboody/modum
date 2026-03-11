use std::path::Path;

use syn::{
    File, Item, ItemMod, ItemUse, UseTree, Visibility,
    spanned::Spanned,
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
        let Some(parent_module) = leaf.full_path.iter().rev().nth(1).cloned() else {
            continue;
        };
        let parent_normalized = parent_module.to_ascii_lowercase();

        let (code, message) = if settings.generic_nouns.contains(binding_name) {
            generic_noun_message(is_reexport, &parent_module, source_name)
        } else if let Some(shorter_leaf) = redundant_leaf_context_candidate(&parent_module, binding_name)
        {
            redundant_context_message(is_reexport, &parent_module, binding_name, &shorter_leaf)
        } else if settings
            .namespace_preserving_modules
            .contains(&parent_normalized)
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

fn redundant_leaf_context_candidate(parent_module: &str, leaf_name: &str) -> Option<String> {
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
