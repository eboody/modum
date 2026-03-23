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
        let mut state = serializer.serialize_struct("Diagnostic", 8)?;
        state.serialize_field("level", &self.level())?;
        state.serialize_field("file", &self.file)?;
        state.serialize_field("line", &self.line)?;
        state.serialize_field("code", &self.code())?;
        state.serialize_field("profile", &self.profile())?;
        state.serialize_field("policy", &self.is_policy_violation())?;
        state.serialize_field("fix", &self.fix)?;
        state.serialize_field("message", &self.message)?;
        state.end()
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
        "internal_redundant_category_suffix" => (
            LintProfile::Core,
            "An internal item leaf repeats the parent category in a redundant suffix.",
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
