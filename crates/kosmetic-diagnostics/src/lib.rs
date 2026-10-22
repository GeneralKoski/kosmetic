//! Spans, diagnostics and rendering shared by every kosmetic crate.
//!
//! Everything that points into the source uses byte offsets ([`Span`]).
//! Line and column numbers are computed only when a diagnostic is rendered,
//! through a [`LineIndex`].

mod edit;
mod line_index;
mod render;
mod span;

pub use edit::apply_fixes;
pub use line_index::{LineCol, LineIndex};
pub use render::{render_human, render_json};
pub use span::Span;

use serde::Serialize;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Something worth knowing, never fails the run.
    Info,
    /// A problem that does not make the file invalid.
    Warning,
    /// The file is invalid TOML or breaks a rule configured as `error`.
    Error,
}

impl Severity {
    /// Lowercase name used in rendered output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// A secondary location attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Label {
    /// Where the label points.
    pub span: Span,
    /// Short text shown next to the location.
    pub message: String,
}

/// A suggested edit that fixes the problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fix {
    /// Range of the source to replace.
    pub span: Span,
    /// Text that replaces the range.
    pub replacement: String,
}

/// A single problem found in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable identifier, for example `syntax/unterminated-string` or
    /// `style/no-trailing-whitespace`.
    pub code: &'static str,
    /// How serious the problem is.
    pub severity: Severity,
    /// One-line description of the problem.
    pub message: String,
    /// Main location of the problem.
    pub span: Span,
    /// Extra locations, for example the first definition of a duplicated key.
    pub labels: Vec<Label>,
    /// Optional hint on how to solve the problem.
    pub help: Option<String>,
    /// Optional automatic fix.
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// Creates a diagnostic with no labels, help or fix.
    pub fn new(
        code: &'static str,
        severity: Severity,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            span,
            labels: Vec::new(),
            help: None,
            fix: None,
        }
    }

    /// Adds a secondary label.
    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a help message.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Adds an automatic fix.
    #[must_use]
    pub fn with_fix(mut self, span: Span, replacement: impl Into<String>) -> Self {
        self.fix = Some(Fix {
            span,
            replacement: replacement.into(),
        });
        self
    }
}
