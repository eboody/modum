use std::path::PathBuf;

use serde::{Serialize, Serializer, ser::SerializeStruct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LintProfile {
    Core,
    Surface,
    #[default]
    Strict,
}

impl LintProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Surface => "surface",
            Self::Strict => "strict",
        }
    }
}

impl std::str::FromStr for LintProfile {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "core" => Ok(Self::Core),
            "surface" => Ok(Self::Surface),
            "strict" => Ok(Self::Strict),
            _ => Err(format!(
                "invalid profile `{raw}`; expected core|surface|strict"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticCodeInfo {
    pub profile: LintProfile,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticClass {
    ToolError,
    ToolWarning,
    PolicyError { code: String },
    PolicyWarning { code: String },
    AdvisoryWarning { code: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticFixKind {
    ReplacePath,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DiagnosticFix {
    pub kind: DiagnosticFixKind,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticGuidance {
    pub why: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    pub class: DiagnosticClass,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub fix: Option<DiagnosticFix>,
    pub message: String,
}

impl Diagnostic {
    pub fn error(file: Option<PathBuf>, line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            class: DiagnosticClass::ToolError,
            file,
            line,
            fix: None,
            message: message.into(),
        }
    }

    pub fn warning(file: Option<PathBuf>, line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            class: DiagnosticClass::ToolWarning,
            file,
            line,
            fix: None,
            message: message.into(),
        }
    }

    pub fn policy(
        file: Option<PathBuf>,
        line: Option<usize>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            class: DiagnosticClass::PolicyWarning { code: code.into() },
            file,
            line,
            fix: None,
            message: message.into(),
        }
    }

    pub fn policy_error(
        file: Option<PathBuf>,
        line: Option<usize>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            class: DiagnosticClass::PolicyError { code: code.into() },
            file,
            line,
            fix: None,
            message: message.into(),
        }
    }

    pub fn advisory(
        file: Option<PathBuf>,
        line: Option<usize>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            class: DiagnosticClass::AdvisoryWarning { code: code.into() },
            file,
            line,
            fix: None,
            message: message.into(),
        }
    }

    pub fn with_fix(mut self, fix: DiagnosticFix) -> Self {
        self.fix = Some(fix);
        self
    }

    pub fn guidance(&self) -> Option<DiagnosticGuidance> {
        let code = self.code()?;
        diagnostic_guidance_for_instance(code, &self.message, self.fix.as_ref())
    }

    pub fn level(&self) -> DiagnosticLevel {
        match self.class {
            DiagnosticClass::ToolError | DiagnosticClass::PolicyError { .. } => {
                DiagnosticLevel::Error
            }
            DiagnosticClass::ToolWarning
            | DiagnosticClass::PolicyWarning { .. }
            | DiagnosticClass::AdvisoryWarning { .. } => DiagnosticLevel::Warning,
        }
    }

    pub fn code(&self) -> Option<&str> {
        match &self.class {
            DiagnosticClass::PolicyError { code }
            | DiagnosticClass::PolicyWarning { code }
            | DiagnosticClass::AdvisoryWarning { code } => Some(code),
            DiagnosticClass::ToolError | DiagnosticClass::ToolWarning => None,
        }
    }

    pub fn profile(&self) -> Option<LintProfile> {
        self.code()
            .and_then(|code| diagnostic_code_info(code).map(|info| info.profile))
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self.class,
            DiagnosticClass::ToolError | DiagnosticClass::PolicyError { .. }
        )
    }

    pub fn is_policy_warning(&self) -> bool {
        matches!(self.class, DiagnosticClass::PolicyWarning { .. })
    }

    pub fn is_advisory_warning(&self) -> bool {
        matches!(
            self.class,
            DiagnosticClass::ToolWarning | DiagnosticClass::AdvisoryWarning { .. }
        )
    }

    pub fn is_policy_violation(&self) -> bool {
        matches!(
            self.class,
            DiagnosticClass::PolicyError { .. } | DiagnosticClass::PolicyWarning { .. }
        )
    }

    pub fn included_in_profile(&self, profile: LintProfile) -> bool {
        match &self.class {
            DiagnosticClass::ToolError | DiagnosticClass::ToolWarning => true,
            DiagnosticClass::PolicyError { code }
            | DiagnosticClass::PolicyWarning { code }
            | DiagnosticClass::AdvisoryWarning { code } => {
                profile >= minimum_profile_for_code(code)
            }
        }
    }
}

impl Serialize for Diagnostic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Diagnostic", 9)?;
        state.serialize_field("level", &self.level())?;
        state.serialize_field("file", &self.file)?;
        state.serialize_field("line", &self.line)?;
        state.serialize_field("code", &self.code())?;
        state.serialize_field("profile", &self.profile())?;
        state.serialize_field("policy", &self.is_policy_violation())?;
        state.serialize_field("fix", &self.fix)?;
        state.serialize_field("guidance", &self.guidance())?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

fn diagnostic_guidance_parts_for_code(code: &str) -> Option<(&'static str, &'static str)> {
    let (why, address) = match code {
        "namespace_flat_use" | "namespace_flat_pub_use" | "namespace_flat_type_alias" => (
            "The imported leaf still depends on the parent path to read clearly, so flattening it makes call sites harder to scan.",
            "Keep the meaningful qualifier visible at the use site or public surface named in the lint. If you want a shorter path, promote a real parent surface first. Don't flatten it and then smuggle the missing meaning back with an alias or a longer leaf.",
        ),
        "namespace_flat_use_preserve_module"
        | "namespace_flat_pub_use_preserve_module"
        | "namespace_flat_type_alias_preserve_module"
        | "namespace_glob_preserve_module" => (
            "The child module is a real facet like `http`, `query`, or `components`, so hiding it erases role information that the path should keep visible.",
            "Import or re-export through the preserved module named in the lint so the facet stays visible. Don't keep the path flat and patch over the loss with a local alias, a glob, or a longer leaf name.",
        ),
        "namespace_flat_use_redundant_leaf_context"
        | "namespace_flat_pub_use_redundant_leaf_context"
        | "namespace_flat_type_alias_redundant_leaf_context"
        | "internal_redundant_leaf_context"
        | "internal_adapter_redundant_leaf_context"
        | "api_redundant_leaf_context" => (
            "The leaf is repeating context that the parent path already supplies, so the name is doing work the module path should already be doing.",
            "Move the repeated context into the path and keep the shorter leaf only if the resulting path is still a complete, standalone name. Don't fake this with a local alias or by mechanically chopping tokens; if the short leaf becomes vague, strengthen the parent path or keep the longer leaf.",
        ),
        "namespace_redundant_qualified_generic" => (
            "The qualifier repeats a generic category the leaf already names, so the written path is longer without adding meaning.",
            "Use the nearer parent surface if it already exists. For owned code, create that parent-surface re-export first, migrate callers to it, and then shorten the written path. Don't silence the lint with a local alias, a compensating leaf rename, or a duplicate shadow surface.",
        ),
        "namespace_overqualified_callsite_path" => (
            "The call site is spelling more of the module scaffolding than it needs, which makes the code heavier without adding comparable meaning.",
            "Import the smallest semantic parent module and call through that instead of keeping the full absolute or deep path inline. Don't flatten all the way to the bare leaf, and don't keep the whole path just because it is technically correct. If the shortest meaningful module still reads badly, the canonical surface likely wants work.",
        ),
        "namespace_aliased_qualified_path" => (
            "The local alias hides the real semantic module path and makes the call site read flatter and more technical than the source surface.",
            "Use the semantic module path directly, or for owned code promote the binding to the nearer parent surface the lint names and use that consistently. Don't keep the technical alias around as the de facto path.",
        ),
        "namespace_family_unsupported_construct" => (
            "This namespace family depends on constructs like macros, cfg gates, or includes that the current source-level pass can't interpret authoritatively.",
            "Treat this as an analysis boundary. Keep the real module path visible until you've verified the family from the expanded or real surface. Don't flatten the path just because the current pass couldn't prove the family, and don't rewrite macros, includes, or cfg-driven code only to satisfy this lint.",
        ),
        "namespace_parent_surface" => (
            "The parent module already exposes the readable caller-facing surface, so reaching through a child module bypasses the intended entrypoint.",
            "Import or re-export the binding from the parent surface named in the lint and make that the canonical caller-facing path. Keep the child path for implementation organization only; don't add the parent alias and then keep callers split across both surfaces.",
        ),
        "namespace_prelude_glob_import" => (
            "A prelude glob hides where names come from, which makes it harder to tell which module is carrying the meaning.",
            "Import the concrete items you need or keep the meaningful module visible at the call site instead of relying on the glob. Don't replace one hidden source with another flatter alias or umbrella prelude.",
        ),
        "internal_catch_all_module" | "api_catch_all_module" => (
            "A bucket module like `util` or `service` forces item names to carry all the meaning because the module itself says almost nothing.",
            "Split the bucket by a real domain or facet, or rename the module to the semantic boundary it actually owns. Don't just move the same mixed contents under another weak bucket like `common`, `shared`, `helpers`, or `types`.",
        ),
        "internal_flat_namespace_preserving_module" => (
            "The flat compound module name is hiding a facet that should stay visible as part of the path.",
            "Reshape the module into the semantic parent and preserved child facet named in the lint, and move the family consistently. Do the filesystem refactor for real: create the actual module files or directories and move the code there instead of mounting the old files through `#[path = ...]`. Don't leave the flat compound module in place and patch over it with longer item names or one-off re-exports.",
        ),
        "internal_organizational_submodule_flatten" => (
            "A pure category module like `errors`, `request`, or `response` is making naming carry the burden instead of the path.",
            "Flatten the family back to the stronger parent surface or rename the module to the actual semantic boundary it owns. If that means changing module layout, create the real files or directories and move the code there; don't keep the old layout behind `#[path = ...]` shims. Don't keep the category module and only rename items inside it, and don't swap it for another organizational bucket.",
        ),
        "internal_path_shim_module" | "api_path_shim_module" => (
            "A `#[path = ...]` module shim makes the namespace say one thing while the filesystem still says another, so the structure only looks fixed.",
            "Create the real module file or directory for the semantic path the code now wants, move or rename the source into it, and remove the `#[path]` attribute. Don't satisfy namespace or semantic-module lints by keeping the old files in place behind a prettier shim.",
        ),
        "internal_redundant_category_suffix" => (
            "The item suffix is repeating the parent category, so the name is noisier without adding meaning.",
            "Drop the repeated category suffix if the parent path already carries it clearly. Don't strip the token mechanically: if the shorter leaf becomes vague or collides, strengthen the parent path or keep the longer leaf.",
        ),
        "api_missing_parent_surface_export" => (
            "The child module has the main binding, but callers still have to reach into that child path instead of using the readable parent surface.",
            "Add the parent-surface re-export the lint is asking for and treat that parent path as the readable caller-facing entrypoint. Don't dodge this by renaming the child module or type, and don't add a second surface while leaving callers on the child path.",
        ),
        "api_anyhow_error_surface" => (
            "A public boundary that leaks `anyhow` hides the crate's real error vocabulary and makes the surface harder to understand and match on.",
            "Expose a crate-owned typed error at the boundary and convert internal `anyhow` failures into it. Keep `anyhow` inside the implementation boundary where it belongs, and don't wrap `anyhow::Error` in a thin alias or pass-through newtype and call that a typed boundary.",
        ),
        "api_string_error_surface" => (
            "A raw string error loses structure, variants, and machine-readable meaning at the API boundary.",
            "Replace the string boundary with a typed error value that names the real failure cases. Don't hide the same free-form text inside a wrapper with one `message` field or defer the modeling work deeper in the workflow.",
        ),
        "api_manual_error_surface" => (
            "The public error shape looks like an ad hoc wrapper instead of a focused typed boundary with named failure cases.",
            "Give the boundary an explicit typed error design that matches what callers need to understand. If the wrapper is only carrying text, replace it with real variants or focused fields instead of polishing the wrapper shell.",
        ),
        "api_semantic_string_scalar" => (
            "The boundary name suggests domain meaning like `url`, `email`, or `path`, but the type is still a raw string.",
            "Parse or validate at the boundary into the focused type the repo wants to use there. If a reusable newtype or domain wrapper already exists, use that instead of renaming the string field or parameter. Don't stop at a plain `type ... = String` alias, a pass-through wrapper, or a later parse step.",
        ),
        "api_semantic_numeric_scalar" => (
            "The boundary name suggests a unit or domain meaning like duration, timestamp, or port, but the type is still a bare number.",
            "Use a typed duration, timestamp, port, or small domain wrapper at the boundary. Don't just rename the field, switch integer widths, or hide the same raw number inside another config object.",
        ),
        "api_raw_id_surface" => (
            "An id at the boundary usually carries validation, formatting, or cross-system meaning that a bare string or integer can't express.",
            "Introduce or reuse a focused id type at the boundary and validate or parse into it there. Avoid silencing the lint by only renaming the raw field, using a plain `type ... = String` alias, or wrapping the same raw value without stronger semantics.",
        ),
        "api_boolean_flag_cluster" => (
            "Several booleans together usually mean the boundary is really describing a smaller mode or policy model.",
            "Replace the cluster with a typed options object or enum that names the actual combinations callers are supposed to choose between. Don't just move the same booleans into another bag or builder and call the boundary modeled.",
        ),
        "api_manual_flag_set" => (
            "Parallel constants and raw bitmask handling usually mean the API is hand-rolling a flags boundary.",
            "Replace the raw integer mask with a typed flags surface or wrapper so the boundary names the allowed combinations directly. Don't keep the same bitmask contract behind helper functions or renamed constants.",
        ),
        "callsite_maybe_some" => (
            "Wrapping a concrete value in `Some(...)` when calling a `maybe_*` API throws away the distinction that method is designed to preserve.",
            "If you already have a concrete value, call the non-`maybe_` setter. Use the `maybe_` form when the caller really is forwarding an `Option<_>`. Don't wrap a value in `Some(...)` just to satisfy the method name.",
        ),
        "api_candidate_semantic_module" | "internal_candidate_semantic_module" => (
            "The sibling family looks like it wants one shared semantic module surface instead of repeating the family marker in every leaf or module name.",
            "Treat this as a design prompt, not an automatic rewrite. Extract the semantic module only if it becomes the real canonical surface for the family and the inner leaves get clearer. If you do it, make it a real refactor: create the module files or directories and move the code instead of keeping the old layout behind `#[path = ...]` shims. Don't create a shadow module while keeping the old flat family equally canonical.",
        ),
        "api_candidate_semantic_module_unsupported_construct" => (
            "This scope contains constructs like macros, cfg gates, or includes that the current source-level pass can't interpret authoritatively.",
            "Treat this as an analysis boundary. Inspect the expanded or real surface manually, or upgrade the observation point, before making structural changes here. Don't rewrite macros, includes, or cfg-driven code just to satisfy the current pass.",
        ),
        "api_organizational_submodule_flatten" => (
            "A pure category module like `error`, `request`, or `response` is leaking category structure into the caller-facing path instead of letting the stronger parent surface carry it.",
            "Flatten the public surface back to the stronger parent path or rename the module to the actual semantic boundary it owns. If that means changing module layout, create the real files or directories and move the code there; don't keep the old layout behind `#[path = ...]` shims.",
        ),
        _ if code.starts_with("namespace_") => (
            "The current path shape is hiding meaning in the wrong place, so readers have to recover structure from longer leaves or flatter aliases.",
            "Move the meaning back into the path the lint points at. Fix the owning surface instead of only renaming the local use site or alias.",
        ),
        _ if code.starts_with("internal_") => (
            "The internal module or item shape is making names carry meaning that the structure should carry instead.",
            "Change the structure the lint points at: split the module, strengthen the parent path, or shorten the repeated leaf only when the resulting path stays clear. Avoid bucket renames and token-chop fixes whose only job is to silence the lint.",
        ),
        _ if code.starts_with("api_") => (
            "The caller-facing surface is exposing a shape that hides the real domain or protocol meaning from readers.",
            "Change the caller-facing boundary itself instead of only renaming the current item. Prefer one canonical surface or stronger boundary type over aliases, wrappers, or duplicate paths that preserve the same shape.",
        ),
        _ => return None,
    };

    Some((why, address))
}

pub fn diagnostic_guidance_for_code(
    code: &str,
    fix: Option<&DiagnosticFix>,
) -> Option<DiagnosticGuidance> {
    let (why, address) = diagnostic_guidance_parts_for_code(code)?;

    Some(DiagnosticGuidance {
        why: why.to_string(),
        address: append_direct_rewrite(address, fix),
    })
}

fn diagnostic_guidance_for_instance(
    code: &str,
    message: &str,
    fix: Option<&DiagnosticFix>,
) -> Option<DiagnosticGuidance> {
    let (base_why, base_address) = diagnostic_guidance_parts_for_code(code)?;
    let mut why = base_why.to_string();
    let mut address = base_address.to_string();

    match code {
        "namespace_flat_use" | "namespace_flat_pub_use" | "namespace_flat_type_alias" => {
            if let Some((instance_why, instance_address)) =
                namespace_flat_context_guidance(code, message, fix)
            {
                why = instance_why;
                address = instance_address;
            }
        }
        "namespace_overqualified_callsite_path" => {
            if let Some((instance_why, instance_address)) = overqualified_callsite_guidance(message)
            {
                why = instance_why;
                address = instance_address;
            }
        }
        "namespace_flat_use_redundant_leaf_context"
        | "namespace_flat_pub_use_redundant_leaf_context"
        | "namespace_flat_type_alias_redundant_leaf_context"
        | "internal_redundant_leaf_context"
        | "internal_adapter_redundant_leaf_context"
        | "api_redundant_leaf_context" => {
            append_instance_address(
                &mut address,
                "If this item is also re-exported or caller-visible elsewhere, reconcile that outer surface too so the family ends up with one intentional path shape instead of an internal-only rename.",
            );
        }
        "api_semantic_string_scalar" => {
            append_instance_address(&mut address, &semantic_string_scalar_address_hint(message));
        }
        "api_semantic_numeric_scalar" => {
            append_instance_address(&mut address, &semantic_numeric_scalar_address_hint(message));
        }
        "api_raw_id_surface" => {
            append_instance_address(&mut address, &raw_id_surface_address_hint(message));
        }
        _ => {}
    }

    Some(DiagnosticGuidance {
        why,
        address: append_direct_rewrite(&address, fix),
    })
}

fn append_direct_rewrite(address: &str, fix: Option<&DiagnosticFix>) -> String {
    let Some(fix) = fix else {
        return address.to_string();
    };
    format!(
        "{address} Once the owning surface is right, the site-level rewrite here is `{}`.",
        fix.replacement
    )
}

fn append_instance_address(address: &mut String, extra: &str) {
    if extra.is_empty() {
        return;
    }
    address.push(' ');
    address.push_str(extra);
}

fn semantic_string_scalar_address_hint(message: &str) -> String {
    let (subject, fields) = message_subject_and_fields(message);
    let mut hints = Vec::new();

    for field in fields {
        let lower = field.to_ascii_lowercase();
        if lower.contains("url") {
            let scoped = scoped_boundary_type_hint(subject.as_deref(), &field);
            hints.push(match scoped {
                Some(scoped) => format!(
                    "For `{field}`, use a real URL boundary type. If the repo already has a matching wrapper, something like `{scoped}` is the right shape; otherwise use `url::Url` or a focused wrapper."
                ),
                None => format!(
                    "For `{field}`, use a real URL boundary type like the repo's existing wrapper or `url::Url`."
                ),
            });
        } else if lower.contains("email") {
            hints.push(format!(
                "For `{field}`, use the repo's email type if it exists, or a focused `Email` wrapper, instead of raw text."
            ));
        } else if lower.contains("path") {
            hints.push(format!(
                "For `{field}`, prefer `std::path::PathBuf` or the repo's path wrapper instead of raw text."
            ));
        }
    }

    hints.join(" ")
}

fn semantic_numeric_scalar_address_hint(message: &str) -> String {
    let (_, fields) = message_subject_and_fields(message);
    let mut hints = Vec::new();

    for field in fields {
        let lower = field.to_ascii_lowercase();
        if looks_like_duration_field(&lower) {
            hints.push(format!(
                "For `{field}`, `std::time::Duration` is the natural boundary type."
            ));
        } else if lower.contains("port") {
            hints.push(format!(
                "For `{field}`, use a focused port newtype, `core::num::NonZeroU16` when zero is invalid, or move it into a typed socket, endpoint, or address config surface instead of leaving it as a bare integer."
            ));
        }
    }

    hints.join(" ")
}

fn namespace_flat_context_guidance(
    code: &str,
    message: &str,
    fix: Option<&DiagnosticFix>,
) -> Option<(String, String)> {
    let chunks = backticked_chunks(message);
    let leaf = chunks.first()?.as_str();
    let preferred = fix
        .map(|fix| fix.replacement.as_str())
        .or_else(|| chunks.get(1).map(|chunk| chunk.as_str()))?;
    let (parent, _) = preferred.rsplit_once("::")?;

    let why = match (code, leaf_looks_context_dependent(leaf)) {
        ("namespace_flat_pub_use", true) => format!(
            "The leaf still needs `{parent}::` to read clearly here, so flattening it makes the caller-facing surface harder to scan."
        ),
        ("namespace_flat_pub_use", false) => format!(
            "`{parent}` is a meaningful facet or family home for this item, so flattening it hides structure the caller-facing surface should keep visible."
        ),
        ("namespace_flat_type_alias", true) => format!(
            "The leaf still needs `{parent}::` to read clearly here, so flattening it makes the aliased type path harder to scan."
        ),
        ("namespace_flat_type_alias", false) => format!(
            "`{parent}` is a meaningful facet or family home for this item, so flattening it hides structure the aliased type path should keep visible."
        ),
        (_, true) => format!(
            "The leaf still needs `{parent}::` to read clearly here, so flattening it makes the path harder to scan at the use site."
        ),
        _ => format!(
            "`{parent}` is a meaningful facet or family home for this item, so flattening it hides structure the path should keep visible."
        ),
    };

    let address = match code {
        "namespace_flat_pub_use" => format!(
            "Re-export through `{preferred}` and make `{parent}` the visible public facet here. Don't keep the flat re-export and try to compensate with an alias, a longer leaf, or a second competing surface."
        ),
        "namespace_flat_type_alias" => format!(
            "Keep `{preferred}` visible in the alias so the type path still shows its owning facet. Don't hide the module inside the alias name or flatten it away and compensate elsewhere."
        ),
        _ => format!(
            "Import `{preferred}` directly and keep `{parent}` visible at call sites. Don't flatten it and try to smuggle the missing structure back with an alias or a longer leaf."
        ),
    };

    Some((why, address))
}

fn overqualified_callsite_guidance(message: &str) -> Option<(String, String)> {
    let chunks = backticked_chunks(message);
    let full_path = chunks.first()?;
    let qualifier = chunks.get(1)?;
    let preferred = chunks.get(2)?;
    let (parent, _) = preferred.rsplit_once("::")?;

    let why = format!(
        "The call site is carrying the full path `{full_path}` even though `{parent}` is enough visible context once that module is imported."
    );
    let address = if message.contains("call through existing") {
        format!(
            "Call through the existing `{qualifier}` namespace and prefer `{preferred}` so the call site keeps the semantic facet without paying the whole absolute path cost. Don't flatten all the way to the bare leaf, and don't keep the full path inline just because it compiles."
        )
    } else {
        format!(
            "Import `{qualifier}` and call through `{preferred}` so the call site keeps the semantic facet without paying the whole absolute path cost. Don't flatten all the way to the bare leaf, and don't keep the full path inline just because it compiles."
        )
    };

    Some((why, address))
}

fn raw_id_surface_address_hint(message: &str) -> String {
    let (subject, fields) = message_subject_and_fields(message);
    let mut hints = Vec::new();

    for field in fields {
        if !field.to_ascii_lowercase().contains("id") {
            continue;
        }

        let scoped = scoped_boundary_type_hint(subject.as_deref(), &field)
            .unwrap_or_else(|| pascalize_identifier(&field));
        hints.push(format!(
            "For `{field}`, use the repo's matching id type if it exists; a type like `{scoped}` is the intended boundary shape."
        ));
    }

    hints.join(" ")
}

fn message_subject_and_fields(message: &str) -> (Option<String>, Vec<String>) {
    let chunks = backticked_chunks(message);
    if chunks.is_empty() {
        return (None, Vec::new());
    }

    let subject = chunks.first().cloned();
    let fields = chunks
        .into_iter()
        .skip(1)
        .filter(|chunk| {
            !chunk.contains("::")
                && chunk
                    .chars()
                    .any(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
        .collect::<Vec<_>>();
    (subject, fields)
}

fn backticked_chunks(message: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut rest = message;

    while let Some(start) = rest.find('`') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('`') else {
            break;
        };
        chunks.push(after_start[..end].to_string());
        rest = &after_start[end + 1..];
    }

    chunks
}

fn scoped_boundary_type_hint(subject: Option<&str>, field: &str) -> Option<String> {
    let prefix = subject.and_then(boundary_subject_scope)?;
    let field_type = pascalize_identifier(field);
    if field_type.starts_with(&prefix) {
        return None;
    }
    Some(format!("{prefix}{field_type}"))
}

fn boundary_subject_scope(subject: &str) -> Option<String> {
    let segments = subject.split("::").collect::<Vec<_>>();
    let owner = match segments.as_slice() {
        [] => return None,
        [single] => *single,
        [.., prev, last]
            if last
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase()) =>
        {
            *prev
        }
        [.., last] => *last,
    };
    let scope = ident_words(owner).last().cloned()?;
    (!scope_word_is_generic(&scope)).then_some(scope)
}

fn pascalize_identifier(raw: &str) -> String {
    ident_words(raw)
        .into_iter()
        .map(|word| {
            let mut chars = word.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut rendered = String::new();
            rendered.push(first.to_ascii_uppercase());
            rendered.push_str(chars.as_str());
            rendered
        })
        .collect::<String>()
}

fn ident_words(raw: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_was_lower_or_digit = false;

    for ch in raw.chars() {
        if matches!(ch, '_' | ':' | '-' | ' ') {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            prev_was_lower_or_digit = false;
            continue;
        }

        if ch.is_ascii_uppercase() && prev_was_lower_or_digit && !current.is_empty() {
            words.push(current.clone());
            current.clear();
        }

        current.push(ch);
        prev_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn looks_like_duration_field(field: &str) -> bool {
    field.contains("timeout")
        || field.contains("interval")
        || field.contains("backoff")
        || field.ends_with("_secs")
        || field.ends_with("_ms")
        || field.ends_with("_millis")
        || field.ends_with("_nanos")
}

fn leaf_looks_context_dependent(leaf: &str) -> bool {
    if leaf
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit())
    {
        return true;
    }

    let words = ident_words(leaf);
    if words.len() <= 1 {
        return true;
    }

    let generic_count = words
        .iter()
        .filter(|word| namespace_context_word_is_generic(word))
        .count();

    generic_count.saturating_mul(2) >= words.len().saturating_add(1)
}

fn namespace_context_word_is_generic(word: &str) -> bool {
    let generic_words = [
        "body",
        "config",
        "content",
        "context",
        "cookie",
        "copy",
        "entry",
        "error",
        "field",
        "flow",
        "id",
        "key",
        "kind",
        "layer",
        "level",
        "log",
        "message",
        "name",
        "pipeline",
        "record",
        "repository",
        "request",
        "response",
        "result",
        "service",
        "session",
        "state",
        "store",
        "target",
        "text",
        "type",
        "value",
    ];
    generic_words.contains(&word.to_ascii_lowercase().as_str())
}

fn scope_word_is_generic(scope: &str) -> bool {
    let generic_scope_words = [
        "adapter", "auth", "client", "config", "entry", "event", "hit", "id", "mock", "param",
        "params", "payload", "profile", "record", "request", "response", "result", "state",
        "vault",
    ];
    let normalized = scope.to_ascii_lowercase();
    generic_scope_words.contains(&normalized.as_str())
}

#[cfg(test)]
mod tests {
    use super::{
        Diagnostic, DiagnosticClass, DiagnosticFix, DiagnosticFixKind, diagnostic_guidance_for_code,
    };

    #[test]
    fn instance_guidance_for_semantic_string_scalar_mentions_repo_shaped_url_type() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "api_semantic_string_scalar".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "public struct `SensitiveSandbox` carries semantic scalar field(s) `base_url` as raw strings; prefer typed boundary values or focused newtypes".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.address.contains("SandboxBaseUrl"));
        assert!(guidance.address.contains("url::Url"));
    }

    #[test]
    fn instance_guidance_for_raw_id_surface_mentions_repo_shaped_id_type() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "api_raw_id_surface".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "public struct `SensitiveSandbox` keeps raw id field(s) `client_id` as strings or primitive integers; prefer id newtypes at the boundary".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.address.contains("SandboxClientId"));
    }

    #[test]
    fn instance_guidance_for_flat_use_generic_leaf_mentions_needed_parent_context() {
        let diag = Diagnostic {
            class: DiagnosticClass::PolicyWarning {
                code: "namespace_flat_use".to_string(),
            },
            file: None,
            line: None,
            fix: Some(DiagnosticFix {
                kind: DiagnosticFixKind::ReplacePath,
                replacement: "types::LogFieldKey".to_string(),
            }),
            message: "flattened import hides namespace context for `LogFieldKey`; prefer `types::LogFieldKey`".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.why.contains("still needs `types::`"));
        assert!(
            guidance
                .address
                .contains("Import `types::LogFieldKey` directly")
        );
    }

    #[test]
    fn instance_guidance_for_flat_use_family_leaf_mentions_facet_visibility() {
        let diag = Diagnostic {
            class: DiagnosticClass::PolicyWarning {
                code: "namespace_flat_use".to_string(),
            },
            file: None,
            line: None,
            fix: Some(DiagnosticFix {
                kind: DiagnosticFixKind::ReplacePath,
                replacement: "viewer::ViewerResolutionFlow".to_string(),
            }),
            message: "flattened import hides namespace context for `ViewerResolutionFlow`; prefer `viewer::ViewerResolutionFlow`".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.why.contains("meaningful facet or family home"));
        assert!(
            guidance
                .address
                .contains("Import `viewer::ViewerResolutionFlow` directly")
        );
    }

    #[test]
    fn instance_guidance_for_flat_pub_use_names_reexport_move_directly() {
        let diag = Diagnostic {
            class: DiagnosticClass::PolicyWarning {
                code: "namespace_flat_pub_use".to_string(),
            },
            file: None,
            line: None,
            fix: Some(DiagnosticFix {
                kind: DiagnosticFixKind::ReplacePath,
                replacement: "layers::RequestLayerFlow".to_string(),
            }),
            message: "flattened re-export hides namespace context for `RequestLayerFlow`; prefer `layers::RequestLayerFlow`".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.why.contains("caller-facing surface"));
        assert!(
            guidance
                .address
                .contains("Re-export through `layers::RequestLayerFlow`")
        );
        assert!(guidance.address.contains("visible public facet"));
    }

    #[test]
    fn instance_guidance_for_overqualified_callsite_names_import_and_preferred_path() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "namespace_overqualified_callsite_path".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "`crate::trace_log::demo::chat::short_hyphenated_text` keeps too much module scaffolding at the call site; import `crate::trace_log::demo::chat` and prefer `chat::short_hyphenated_text`".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.why.contains("`chat` is enough visible context"));
        assert!(guidance.address.contains(
            "Import `crate::trace_log::demo::chat` and call through `chat::short_hyphenated_text`"
        ));
    }

    #[test]
    fn instance_guidance_for_overqualified_callsite_reuses_existing_namespace_binding() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "namespace_overqualified_callsite_path".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "`domain::chat::room::name::Error` keeps too much module scaffolding at the call site; call through existing `chat` namespace and prefer `chat::room::name::Error`".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(
            guidance
                .why
                .contains("`chat::room::name` is enough visible context")
        );
        assert!(guidance.address.contains(
            "Call through the existing `chat` namespace and prefer `chat::room::name::Error`"
        ));
    }

    #[test]
    fn instance_guidance_skips_generic_scope_prefixes_for_string_scalars() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "api_semantic_string_scalar".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "public struct `EpicConfig` carries semantic scalar field(s) `base_url` as raw strings; prefer typed boundary values or focused newtypes".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.address.contains("url::Url"));
        assert!(!guidance.address.contains("ConfigBaseUrl"));
    }

    #[test]
    fn instance_guidance_skips_duplicate_scope_prefixes_for_id_types() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "api_raw_id_surface".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "public struct `AuditRecord` keeps raw id field(s) `record_id` as strings or primitive integers; prefer id newtypes at the boundary".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.address.contains("RecordId"));
        assert!(!guidance.address.contains("RecordRecordId"));
    }

    #[test]
    fn generic_guidance_for_unsupported_construct_reports_analysis_boundary() {
        let guidance = diagnostic_guidance_for_code(
            "api_candidate_semantic_module_unsupported_construct",
            None,
        )
        .expect("guidance");
        assert!(guidance.why.contains("can't interpret authoritatively"));
        assert!(guidance.address.contains("analysis boundary"));
        assert!(!guidance.address.contains("Change the public boundary"));
        assert!(guidance.address.contains("Don't rewrite macros"));
    }

    #[test]
    fn generic_guidance_for_namespace_family_unsupported_construct_reports_analysis_boundary() {
        let guidance = diagnostic_guidance_for_code("namespace_family_unsupported_construct", None)
            .expect("guidance");
        assert!(guidance.why.contains("can't interpret authoritatively"));
        assert!(guidance.address.contains("analysis boundary"));
        assert!(
            guidance
                .address
                .contains("Keep the real module path visible")
        );
        assert!(guidance.address.contains("Don't flatten the path"));
    }

    #[test]
    fn generic_guidance_for_parent_surface_warns_against_split_canonical_paths() {
        let guidance =
            diagnostic_guidance_for_code("namespace_parent_surface", None).expect("guidance");
        assert!(guidance.address.contains("canonical caller-facing path"));
        assert!(
            guidance
                .address
                .contains("callers split across both surfaces")
        );
    }

    #[test]
    fn generic_guidance_for_redundant_leaf_context_warns_against_alias_and_token_chop() {
        let guidance = diagnostic_guidance_for_code("internal_redundant_leaf_context", None)
            .expect("guidance");
        assert!(guidance.address.contains("local alias"));
        assert!(guidance.address.contains("mechanically chopping tokens"));
    }

    #[test]
    fn instance_guidance_for_redundant_leaf_context_mentions_outer_surface_reconciliation() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "internal_redundant_leaf_context".to_string(),
            },
            file: None,
            line: None,
            fix: Some(DiagnosticFix {
                kind: DiagnosticFixKind::ReplacePath,
                replacement: "sensitive_boundary::config::SandboxHttp".to_string(),
            }),
            message: "internal item `sensitive_boundary::config::SandboxHttpConfig` repeats the `config` context; prefer `sensitive_boundary::config::SandboxHttp`".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.address.contains("caller-visible elsewhere"));
    }

    #[test]
    fn generic_guidance_for_semantic_string_scalar_warns_against_type_alias_string() {
        let guidance =
            diagnostic_guidance_for_code("api_semantic_string_scalar", None).expect("guidance");
        assert!(guidance.address.contains("type ... = String"));
    }

    #[test]
    fn instance_guidance_for_numeric_port_scalar_mentions_concrete_port_shapes() {
        let diag = Diagnostic {
            class: DiagnosticClass::AdvisoryWarning {
                code: "api_semantic_numeric_scalar".to_string(),
            },
            file: None,
            line: None,
            fix: None,
            message: "public struct `Sensitive` carries semantic scalar field(s) `provider_stub_port` as raw integers; prefer typed duration, timestamp, port, or domain-specific scalar types".to_string(),
        };

        let guidance = diag.guidance().expect("guidance");
        assert!(guidance.address.contains("NonZeroU16"));
        assert!(guidance.address.contains("typed socket"));
    }

    #[test]
    fn generic_guidance_for_candidate_semantic_module_warns_against_shadow_surface() {
        let guidance =
            diagnostic_guidance_for_code("api_candidate_semantic_module", None).expect("guidance");
        assert!(guidance.address.contains("design prompt"));
        assert!(guidance.address.contains("shadow module"));
        assert!(guidance.address.contains("#[path = ...]"));
    }

    #[test]
    fn generic_guidance_for_flat_namespace_module_warns_against_path_shims() {
        let guidance =
            diagnostic_guidance_for_code("internal_flat_namespace_preserving_module", None)
                .expect("guidance");
        assert!(guidance.address.contains("#[path = ...]"));
        assert!(guidance.address.contains("filesystem refactor"));
    }

    #[test]
    fn generic_guidance_for_path_shim_module_demands_real_module_layout() {
        let guidance =
            diagnostic_guidance_for_code("internal_path_shim_module", None).expect("guidance");
        assert!(guidance.why.contains("filesystem"));
        assert!(guidance.address.contains("real module file or directory"));
        assert!(guidance.address.contains("remove the `#[path]` attribute"));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSelection {
    All,
    Policy,
    Advisory,
}

impl DiagnosticSelection {
    pub fn includes(self, diagnostic: &Diagnostic) -> bool {
        match self {
            Self::All => true,
            Self::Policy => diagnostic.is_error() || diagnostic.is_policy_violation(),
            Self::Advisory => diagnostic.is_error() || diagnostic.is_advisory_warning(),
        }
    }

    pub fn report_label(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Policy => Some("policy diagnostics and errors only"),
            Self::Advisory => Some("advisory diagnostics and errors only"),
        }
    }
}

pub fn diagnostic_code_info(code: &str) -> Option<DiagnosticCodeInfo> {
    let (profile, summary) = match code {
        "namespace_flat_use" => (
            LintProfile::Core,
            "Flattened imports hide useful namespace context for leaves that still need the parent path.",
        ),
        "namespace_flat_use_preserve_module" => (
            LintProfile::Core,
            "Flattened imports hide a module that should stay visible at call sites.",
        ),
        "namespace_flat_use_redundant_leaf_context" => (
            LintProfile::Core,
            "Flattened imports keep parent context in the leaf instead of the path.",
        ),
        "namespace_redundant_qualified_generic" => (
            LintProfile::Core,
            "Qualified paths repeat a generic category that the leaf already names.",
        ),
        "namespace_overqualified_callsite_path" => (
            LintProfile::Strict,
            "A long qualified call-site path keeps more module scaffolding visible than the caller needs.",
        ),
        "namespace_aliased_qualified_path" => (
            LintProfile::Core,
            "A namespace alias flattens a semantic path instead of keeping the real module visible.",
        ),
        "namespace_family_unsupported_construct" => (
            LintProfile::Strict,
            "Namespace-family call-site guidance was skipped because source-level analysis hit macros, cfg gates, or includes.",
        ),
        "namespace_parent_surface" => (
            LintProfile::Core,
            "Imports bypass a canonical parent surface that already re-exports the binding.",
        ),
        "namespace_flat_type_alias" => (
            LintProfile::Core,
            "A type alias hides useful namespace context for an aliased leaf that still needs the parent path.",
        ),
        "namespace_flat_type_alias_preserve_module" => (
            LintProfile::Core,
            "A type alias hides a module that should stay visible in the aliased type path.",
        ),
        "namespace_flat_type_alias_redundant_leaf_context" => (
            LintProfile::Core,
            "A type alias keeps redundant parent context in the alias name instead of the path.",
        ),
        "namespace_prelude_glob_import" => (
            LintProfile::Core,
            "A prelude glob import hides the real source modules instead of keeping useful namespace context visible.",
        ),
        "namespace_glob_preserve_module" => (
            LintProfile::Core,
            "A glob import flattens a configured namespace-preserving module instead of keeping that module visible.",
        ),
        "internal_catch_all_module" => (
            LintProfile::Core,
            "An internal module name is a catch-all bucket instead of a stable domain or facet.",
        ),
        "internal_repeated_module_segment" => (
            LintProfile::Core,
            "An internal nested module repeats the same segment instead of adding meaning.",
        ),
        "internal_organizational_submodule_flatten" => (
            LintProfile::Core,
            "An internal organizational module leaks category structure that should usually be flattened.",
        ),
        "internal_weak_module_generic_leaf" => (
            LintProfile::Core,
            "An internal item leaf is too generic for a weak or technical parent module.",
        ),
        "internal_redundant_leaf_context" => (
            LintProfile::Core,
            "An internal item leaf repeats context the parent module already provides.",
        ),
        "internal_adapter_redundant_leaf_context" => (
            LintProfile::Core,
            "An internal adapter leaf repeats implementation context the parent module already provides.",
        ),
        "internal_redundant_category_suffix" => (
            LintProfile::Core,
            "An internal item leaf repeats the parent category in a redundant suffix.",
        ),
        "internal_flat_namespace_preserving_module" => (
            LintProfile::Core,
            "An internal flat module name hides a namespace-preserving facet that should stay visible in the path.",
        ),
        "internal_path_shim_module" => (
            LintProfile::Core,
            "An internal module is being mounted through `#[path = ...]` instead of living at a real module path.",
        ),
        "internal_candidate_semantic_module" => (
            LintProfile::Strict,
            "A family of sibling internal items or modules suggests a stronger semantic module surface.",
        ),
        "api_catch_all_module" => (
            LintProfile::Core,
            "A surface-visible module is a catch-all bucket instead of a stable domain or facet.",
        ),
        "api_repeated_module_segment" => (
            LintProfile::Core,
            "A surface-visible nested module repeats the same segment instead of adding meaning.",
        ),
        "namespace_flat_pub_use" => (
            LintProfile::Surface,
            "A re-export flattens useful namespace context out of the caller-facing path.",
        ),
        "namespace_flat_pub_use_preserve_module" => (
            LintProfile::Surface,
            "A re-export hides a module that should stay visible in the caller-facing path.",
        ),
        "namespace_flat_pub_use_redundant_leaf_context" => (
            LintProfile::Surface,
            "A re-export keeps parent context in the leaf instead of the path.",
        ),
        "api_missing_parent_surface_export" => (
            LintProfile::Surface,
            "A child module surface should usually also expose a readable parent binding.",
        ),
        "api_anyhow_error_surface" => (
            LintProfile::Surface,
            "A caller-facing surface leaks `anyhow` instead of exposing a crate-owned typed error boundary.",
        ),
        "api_semantic_string_scalar" => (
            LintProfile::Surface,
            "A caller-facing semantic scalar is kept as a raw string instead of a typed boundary value.",
        ),
        "api_semantic_numeric_scalar" => (
            LintProfile::Surface,
            "A caller-facing semantic scalar is kept as a raw integer instead of a typed boundary value.",
        ),
        "api_weak_module_generic_leaf" => (
            LintProfile::Surface,
            "A surface-visible item leaf is too generic for a weak or technical parent module.",
        ),
        "api_redundant_leaf_context" => (
            LintProfile::Surface,
            "A surface-visible item leaf repeats context the parent module already provides.",
        ),
        "api_redundant_category_suffix" => (
            LintProfile::Surface,
            "A surface-visible item leaf repeats the parent category in a redundant suffix.",
        ),
        "api_organizational_submodule_flatten" => (
            LintProfile::Surface,
            "A surface-visible organizational module should usually be flattened out of the path.",
        ),
        "api_path_shim_module" => (
            LintProfile::Core,
            "A surface-visible module is being mounted through `#[path = ...]` instead of living at a real module path.",
        ),
        "api_candidate_semantic_module" => (
            LintProfile::Strict,
            "A family of sibling items suggests a stronger semantic module surface.",
        ),
        "api_candidate_semantic_module_unsupported_construct" => (
            LintProfile::Strict,
            "Semantic-module family inference was skipped because the parsed source contains unsupported constructs.",
        ),
        "api_manual_enum_string_helper" => (
            LintProfile::Strict,
            "A public enum exposes manual string helpers that should usually be standard traits or derives.",
        ),
        "api_ad_hoc_parse_helper" => (
            LintProfile::Strict,
            "A public enum parsing helper should usually be modeled as `FromStr` or `TryFrom<&str>`.",
        ),
        "api_parallel_enum_metadata_helper" => (
            LintProfile::Strict,
            "Parallel enum metadata helpers suggest a typed descriptor surface instead of repeated matches.",
        ),
        "api_strum_serialize_all_candidate" => (
            LintProfile::Strict,
            "Per-variant `strum` strings could be replaced by one enum-level `serialize_all` rule.",
        ),
        "api_builder_candidate" => (
            LintProfile::Strict,
            "A configuration-heavy entrypoint would read better as a builder or typed options surface.",
        ),
        "api_repeated_parameter_cluster" => (
            LintProfile::Strict,
            "Several entrypoints repeat the same positional parameter cluster instead of sharing a typed shape.",
        ),
        "api_optional_parameter_builder" => (
            LintProfile::Strict,
            "Optional positional parameters suggest a builder so callers can omit unset values.",
        ),
        "api_defaulted_optional_parameter" => (
            LintProfile::Strict,
            "Defaulted optional positional parameters suggest a builder rather than `None`-passing.",
        ),
        "callsite_maybe_some" => (
            LintProfile::Strict,
            "A `maybe_*` call wraps a direct value in `Some(...)` instead of using the direct setter or forwarding an existing option.",
        ),
        "api_standalone_builder_surface" => (
            LintProfile::Strict,
            "Parallel `with_*` or `set_*` free functions suggest a real builder surface.",
        ),
        "api_boolean_protocol_decision" => (
            LintProfile::Strict,
            "A boolean encodes a domain or protocol decision that should usually be typed.",
        ),
        "api_boolean_flag_cluster" => (
            LintProfile::Strict,
            "Several booleans jointly shape behavior and suggest a typed mode or options surface.",
        ),
        "api_forwarding_compat_wrapper" => (
            LintProfile::Strict,
            "A helper only forwards to an existing standard conversion trait.",
        ),
        "api_string_error_surface" => (
            LintProfile::Strict,
            "A caller-facing error surface is carried as raw strings instead of a typed error boundary.",
        ),
        "api_manual_error_surface" => (
            LintProfile::Strict,
            "A public error manually exposes formatting and error boilerplate instead of a smaller typed boundary.",
        ),
        "api_raw_key_value_bag" => (
            LintProfile::Strict,
            "A caller-facing metadata or bag surface is modeled as raw string key-value pairs instead of a typed shape.",
        ),
        "api_stringly_protocol_collection" => (
            LintProfile::Strict,
            "Protocol or state collections are modeled as raw strings instead of typed values.",
        ),
        "api_stringly_protocol_parameter" => (
            LintProfile::Strict,
            "A boundary takes protocol or state descriptors as raw strings instead of typed values.",
        ),
        "api_stringly_model_scaffold" => (
            LintProfile::Strict,
            "A model carries semantic descriptor fields as raw strings instead of typed structure.",
        ),
        "api_integer_protocol_parameter" => (
            LintProfile::Strict,
            "A caller-facing protocol concept is modeled as a raw integer instead of a typed enum or newtype.",
        ),
        "api_raw_id_surface" => (
            LintProfile::Strict,
            "A caller-facing id is modeled as a raw string or primitive integer instead of a typed id value.",
        ),
        "api_manual_flag_set" => (
            LintProfile::Strict,
            "Parallel integer flag constants suggest a typed flags boundary instead of manual bit masks.",
        ),
        _ => return None,
    };

    Some(DiagnosticCodeInfo { profile, summary })
}

fn minimum_profile_for_code(code: &str) -> LintProfile {
    diagnostic_code_info(code)
        .map(|info| info.profile)
        .unwrap_or(LintProfile::Strict)
}

impl std::str::FromStr for DiagnosticSelection {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "all" => Ok(Self::All),
            "policy" => Ok(Self::Policy),
            "advisory" => Ok(Self::Advisory),
            _ => Err(format!(
                "invalid show mode `{raw}`; expected all|policy|advisory"
            )),
        }
    }
}
