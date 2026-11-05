//! Diagnostics shared by the parser, the engine and the lint rules (ADR 0008).

use serde::Serialize;

/// How serious a diagnostic is. Ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational hint.
    Info,
    /// Likely problem; the plan still schedules.
    Warn,
    /// The plan is invalid or breaks a rule configured as an error.
    Error,
}

impl Severity {
    /// Lowercase name used in files and output.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Error => "error",
        }
    }
}

/// A position in the plan file (1-based line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    /// 1-based line.
    pub line: usize,
    /// 1-based column, in characters.
    pub col: usize,
}

/// One finding about a plan: a stable code, a message, the ids involved and an optional fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable code, `E-<AREA>-nn` or `L-<AREA>-nn`.
    pub code: &'static str,
    /// Severity after applying the plan's rule configuration.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// Ids of the objects involved, most relevant first.
    pub ids: Vec<String>,
    /// Where in the file, when the diagnostic points at the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// A command that fixes the problem, only when it is safe to run blind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    /// A hint for a fix that needs judgment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Diagnostic {
    /// A hard error (`E-*` code).
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    /// A diagnostic with an explicit severity.
    pub fn new(code: &'static str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            ids: Vec::new(),
            span: None,
            fix: None,
            hint: None,
        }
    }

    /// Adds the ids involved.
    pub fn with_ids<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ids.extend(ids.into_iter().map(Into::into));
        self
    }

    /// Sets the file position.
    pub fn with_span(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }

    /// Sets a safe fix command.
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    /// Sets a hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// True when any diagnostic is an error.
pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}
