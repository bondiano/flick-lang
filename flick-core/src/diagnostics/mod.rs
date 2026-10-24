//! Diagnostic model: stable `FLICK-` code, severity, message, UTF-8 byte
//! span, related spans and fix suggestions.

pub mod codes;
pub mod render;
pub mod suggest;

pub use codes::Code;

use crate::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    /// `None` for project/CLI diagnostics that have no source location.
    pub span: Option<Span>,
    pub related: Vec<Related>,
    pub fixes: Vec<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub message: String,
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

impl Diagnostic {
    pub fn new(code: Code, severity: Severity, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity,
            message: message.into(),
            span: None,
            related: Vec::new(),
            fixes: Vec::new(),
        }
    }

    pub fn error(code: Code, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(code, Severity::Error, message).with_span(span)
    }

    /// A warning about code nothing uses: an import, a variable, a private
    /// definition. Editors grey such code out.
    pub fn unnecessary(&self) -> bool {
        [codes::R012, codes::R013, codes::R014].contains(&self.code)
    }

    #[must_use]
    pub fn with_span(self, span: Span) -> Self {
        Diagnostic {
            span: Some(span),
            ..self
        }
    }

    #[must_use]
    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related.push(Related {
            span,
            message: message.into(),
        });
        self
    }

    #[must_use]
    pub fn with_fix(mut self, message: impl Into<String>, edits: Vec<Edit>) -> Self {
        self.fixes.push(Fix {
            message: message.into(),
            edits,
        });
        self
    }
}
