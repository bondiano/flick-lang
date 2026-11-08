//! What a plugin finds wrong in a block, and the fixes it offers.

use serde::{Deserialize, Serialize};

use crate::Span;

/// What is wrong in a block; `code` is the plugin's own, such as `L001`,
/// which the compiler shows under the plugin's package: `std/L001`. Only one
/// with a code may be a warning or a note, and carry fixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub code: Option<String>,
    pub severity: Severity,
    pub fixes: Vec<Fix>,
}

/// How bad a diagnostic is: an error fails the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A change an editor offers for a diagnostic: `edits` replace spans of the
/// block, and a `safe` fix is one `flick check --fix` applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    pub message: String,
    pub edits: Vec<Edit>,
    pub safe: bool,
}

/// The text that replaces a span of the block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

impl Diagnostic {
    /// An error without a code of the plugin.
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Diagnostic {
            message: message.into(),
            span,
            code: None,
            severity: Severity::Error,
            fixes: Vec::new(),
        }
    }

    // shortcut: `expand` gives code or errors, so a warning comes from `check` or
    // beside a refusal; add a `report` import when a plugin warns as it expands
    /// A warning without a code of the plugin; give it one with
    /// [`Self::with_code`], or the compiler reports it as an error.
    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            ..Self::error(message, span)
        }
    }

    /// This diagnostic with the fix `message` that makes `edits`; with
    /// `safe`, `flick check --fix` applies it.
    #[must_use]
    pub fn with_fix(mut self, message: impl Into<String>, edits: Vec<Edit>, safe: bool) -> Self {
        self.fixes.push(Fix {
            message: message.into(),
            edits,
            safe,
        });
        self
    }

    /// This error under the plugin's code `code`, such as `L001`.
    #[must_use]
    pub fn with_code(self, code: &str) -> Self {
        Diagnostic {
            code: Some(code.to_owned()),
            ..self
        }
    }
}
