use std::path::PathBuf;

use serde::{Serialize, Serializer, ser::SerializeStruct};

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
}

impl Serialize for Diagnostic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Diagnostic", 7)?;
        state.serialize_field("level", &self.level())?;
        state.serialize_field("file", &self.file)?;
        state.serialize_field("line", &self.line)?;
        state.serialize_field("code", &self.code())?;
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
