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
            "The imported leaf is generic enough that the path is carrying useful meaning; flattening it makes call sites harder to scan.",
            "Keep the meaningful qualifier visible at the use site or public surface named in the lint. If the qualified form would still be redundant, leave it flat instead of forcing noise.",
        ),
        "namespace_flat_use_preserve_module"
        | "namespace_flat_pub_use_preserve_module"
        | "namespace_flat_type_alias_preserve_module"
        | "namespace_glob_preserve_module" => (
            "The child module is a real facet like `http`, `query`, or `components`, so hiding it erases role information that the path should keep visible.",
            "Import or re-export through the preserved module named in the lint so the facet stays visible. Don't compensate by stuffing that missing context into a longer leaf name.",
        ),
        "namespace_flat_use_redundant_leaf_context"
        | "namespace_flat_pub_use_redundant_leaf_context"
        | "namespace_flat_type_alias_redundant_leaf_context"
        | "internal_redundant_leaf_context"
        | "internal_adapter_redundant_leaf_context"
        | "api_redundant_leaf_context" => (
            "The leaf is repeating context that the parent path already supplies, so the name is doing work the module path should already be doing.",
            "Move the repeated context into the path and keep the shorter leaf only if the resulting path still reads clearly. If the short leaf would become vague, strengthen the module path instead of cargo-culting the shorter name.",
        ),
        "namespace_redundant_qualified_generic" => (
            "The qualifier repeats a generic category the leaf already names, so the written path is longer without adding meaning.",
            "Use the nearer parent surface if it already exists. For owned code, create that parent-surface re-export first and then use it. Don't silence the lint with a meaningless rename that preserves the same shape.",
        ),
        "namespace_aliased_qualified_path" => (
            "The local alias hides the real semantic module path and makes the call site read flatter and more technical than the source surface.",
            "Use the semantic module path directly, or for owned code promote the binding to the nearer parent surface the lint names. Avoid technical alias names that only hide the real structure.",
        ),
        "namespace_parent_surface" => (
            "The parent module already exposes the readable caller-facing surface, so reaching through a child module bypasses the intended entrypoint.",
            "Import or re-export the binding from the parent surface named in the lint instead of reaching into the child module. Keep the child path for implementation organization, not for the main caller-facing path.",
        ),
        "namespace_prelude_glob_import" => (
            "A prelude glob hides where names come from, which makes it harder to tell which module is carrying the meaning.",
            "Import the concrete items you need or keep the meaningful module visible at the call site instead of relying on the glob.",
        ),
        "internal_catch_all_module" | "api_catch_all_module" => (
            "A bucket module like `util` or `service` forces item names to carry all the meaning because the module itself says almost nothing.",
            "Split the bucket by a real domain or facet, or rename the module to the semantic boundary it actually owns. Don't just move the same mixed contents under another weak bucket name.",
        ),
        "internal_flat_namespace_preserving_module" => (
            "The flat compound module name is hiding a facet that should stay visible as part of the path.",
            "Reshape the module into the semantic parent and preserved child facet named in the lint. Fix the structure rather than patching over it with longer item names inside the flat module.",
        ),
        "internal_organizational_submodule_flatten" => (
            "A pure category module like `errors`, `request`, or `response` is making naming carry the burden instead of the path.",
            "Flatten the family back to the stronger parent surface or rename the module to the actual semantic boundary it owns. Don't keep the category module and only rename the items inside it.",
        ),
        "internal_redundant_category_suffix" => (
            "The item suffix is repeating the parent category, so the name is noisier without adding meaning.",
            "Drop the repeated category suffix if the parent path already carries it clearly. If that produces an unclear leaf, improve the parent path instead of keeping the redundant suffix.",
        ),
        "api_missing_parent_surface_export" => (
            "The child module has the main binding, but callers still have to reach into that child path instead of using the readable parent surface.",
            "Add the parent-surface re-export the lint is asking for so callers can use the semantic parent path. Don't dodge this by renaming the child module or type with scaffolding words just to quiet the warning.",
        ),
        "api_anyhow_error_surface" => (
            "A public boundary that leaks `anyhow` hides the crate's real error vocabulary and makes the surface harder to understand and match on.",
            "Expose a crate-owned typed error at the boundary and convert internal `anyhow` failures into it. Keep `anyhow` inside the implementation boundary where it belongs.",
        ),
        "api_string_error_surface" => (
            "A raw string error loses structure, variants, and machine-readable meaning at the API boundary.",
            "Replace the string boundary with a typed error value. If the lint is on a parsing or protocol edge, model the actual failure cases instead of collapsing them into free-form text.",
        ),
        "api_manual_error_surface" => (
            "The public error shape looks like an ad hoc wrapper instead of a focused typed boundary with named failure cases.",
            "Give the boundary an explicit typed error design that matches what callers need to understand. If the wrapper is only carrying text, replace it with a real error type instead of polishing the wrapper.",
        ),
        "api_semantic_string_scalar" => (
            "The boundary name suggests domain meaning like `url`, `email`, or `path`, but the type is still a raw string.",
            "Parse or validate at the boundary into the focused type the repo wants to use there. If a reusable newtype or domain wrapper already exists, use that instead of renaming the string field or parameter.",
        ),
        "api_semantic_numeric_scalar" => (
            "The boundary name suggests a unit or domain meaning like duration, timestamp, or port, but the type is still a bare number.",
            "Use a typed duration, timestamp, port, or small domain wrapper at the boundary. Don't just rename the field while leaving the raw scalar contract unchanged.",
        ),
        "api_raw_id_surface" => (
            "An id at the boundary usually carries validation, formatting, or cross-system meaning that a bare string or integer can't express.",
            "Introduce or reuse a focused id type at the boundary and validate or parse into it there. Avoid silencing the lint by only renaming the raw field.",
        ),
        "api_boolean_flag_cluster" => (
            "Several booleans together usually mean the boundary is really describing a smaller mode or policy model.",
            "Replace the cluster with a typed options object or enum that names the actual combinations callers are supposed to choose between.",
        ),
        "api_manual_flag_set" => (
            "Parallel constants and raw bitmask handling usually mean the API is hand-rolling a flags boundary.",
            "Replace the raw integer mask with a typed flags surface or wrapper so the boundary names the allowed flag combinations directly.",
        ),
        "callsite_maybe_some" => (
            "Wrapping a concrete value in `Some(...)` when calling a `maybe_*` API throws away the distinction that method is designed to preserve.",
            "If you already have a concrete value, call the non-`maybe_` setter. Use the `maybe_` form when the caller really is forwarding an `Option<_>`.",
        ),
        "api_candidate_semantic_module" | "internal_candidate_semantic_module" => (
            "The sibling family looks like it wants one shared semantic module surface instead of repeating the family marker in every leaf or module name.",
            "Treat this as a design prompt, not a mandatory rewrite. Extract the semantic module only if it makes the resulting paths clearer and more stable for readers.",
        ),
        "api_candidate_semantic_module_unsupported_construct" => (
            "This scope contains constructs like macros, cfg gates, or includes that the current source-level pass can't interpret authoritatively.",
            "Treat this as an analysis boundary, not a rewrite prompt. Inspect the expanded or real public surface manually, or upgrade the observation point, before making structural changes here.",
        ),
        _ if code.starts_with("namespace_") => (
            "The current path shape is hiding meaning in the wrong place, so readers have to recover structure from longer leaves or flatter aliases.",
            "Move the meaning back into the path the lint points at. Prefer a clearer module surface over a rename whose only job is to silence the lint.",
        ),
        _ if code.starts_with("internal_") => (
            "The internal module or item shape is making names carry meaning that the structure should carry instead.",
            "Change the structure the lint points at: split the module, strengthen the parent path, or shorten the repeated leaf only when the resulting path still reads clearly.",
        ),
        _ if code.starts_with("api_") => (
            "The caller-facing surface is exposing a shape that hides the real domain or protocol meaning from readers.",
            "Change the public boundary itself instead of only renaming the current item. The better fix is usually a clearer parent path, a re-export, or a stronger boundary type.",
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
    let (why, base_address) = diagnostic_guidance_parts_for_code(code)?;
    let mut address = base_address.to_string();

    match code {
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
        why: why.to_string(),
        address: append_direct_rewrite(&address, fix),
    })
}

fn append_direct_rewrite(address: &str, fix: Option<&DiagnosticFix>) -> String {
    let Some(fix) = fix else {
        return address.to_string();
    };
    format!(
        "{address} For this site, the direct rewrite is `{}`.",
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
                "For `{field}`, use a focused port wrapper or move it into a typed address or config surface instead of leaving it as a bare integer."
            ));
        }
    }

    hints.join(" ")
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
    use super::{Diagnostic, DiagnosticClass, diagnostic_guidance_for_code};

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
            "Flattened imports hide useful namespace context for generic leaves.",
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
        "namespace_aliased_qualified_path" => (
            LintProfile::Core,
            "A namespace alias flattens a semantic path instead of keeping the real module visible.",
        ),
        "namespace_parent_surface" => (
            LintProfile::Core,
            "Imports bypass a canonical parent surface that already re-exports the binding.",
        ),
        "namespace_flat_type_alias" => (
            LintProfile::Core,
            "A type alias hides useful namespace context for a generic aliased leaf.",
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
