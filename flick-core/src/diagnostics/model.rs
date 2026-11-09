use crate::source::Span;

use super::{Code, codes};

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
    /// The checker relied on an error type while forming this diagnostic.
    pub(crate) incomplete: bool,
    #[cfg(feature = "diagnostic-fixes")]
    pub fixes: Vec<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    pub span: Span,
    pub message: String,
}

#[cfg(feature = "diagnostic-fixes")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub message: String,
    pub edits: Vec<Edit>,
    /// Keeps the program's meaning, so `flick check --fix` applies it.
    pub safe: bool,
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
            incomplete: false,
            #[cfg(feature = "diagnostic-fixes")]
            fixes: Vec::new(),
        }
    }

    pub fn error(code: Code, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(code, Severity::Error, message).with_span(span)
    }

    /// A warning about code nothing uses: an import, a variable, a private
    /// definition. Editors grey such code out.
    pub fn unnecessary(&self) -> bool {
        [
            codes::R012,
            codes::R013,
            codes::R014,
            codes::R017,
            codes::T059,
        ]
        .contains(&self.code)
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

    /// `d` with another severity, keeping its other fields.
    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }
    #[must_use]
    pub(crate) fn with_incomplete(mut self, incomplete: bool) -> Self {
        self.incomplete = incomplete;
        self
    }

    /// Builds a suggested fix only when diagnostic fixes are enabled.
    #[must_use]
    pub fn with_fix<M: Into<String>>(self, fix: impl FnOnce() -> (M, Vec<Edit>)) -> Self {
        self.attach_fix(false, fix)
    }

    /// [`Self::with_fix`] for a fix that `flick check --fix` applies.
    #[must_use]
    pub fn with_safe_fix<M: Into<String>>(self, fix: impl FnOnce() -> (M, Vec<Edit>)) -> Self {
        self.attach_fix(true, fix)
    }

    fn attach_fix<M: Into<String>>(self, safe: bool, fix: impl FnOnce() -> (M, Vec<Edit>)) -> Self {
        #[cfg(feature = "diagnostic-fixes")]
        {
            let mut diagnostic = self;
            let (message, edits) = fix();
            diagnostic.fixes.push(Fix {
                message: message.into(),
                edits,
                safe,
            });
            diagnostic
        }
        #[cfg(not(feature = "diagnostic-fixes"))]
        {
            let _ = (safe, fix);
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;
    use std::cell::Cell;

    #[test]
    fn fix_factories_preserve_the_primary_diagnostic() {
        let called = Cell::new(0);
        let span = Span::new(FileId::default(), 1, 4);
        let diagnostic = Diagnostic::error(codes::T001, "cannot find value `totl`", span)
            .with_related(span, "used here")
            .with_fix(|| {
                called.set(called.get() + 1);
                (
                    "did you mean `total`?",
                    vec![Edit {
                        span,
                        replacement: "total".into(),
                    }],
                )
            })
            .with_safe_fix(|| {
                called.set(called.get() + 1);
                (
                    "remove the annotation",
                    vec![Edit {
                        span,
                        replacement: String::new(),
                    }],
                )
            });
        assert_eq!(diagnostic.code, codes::T001);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.message, "cannot find value `totl`");
        assert_eq!(diagnostic.span, Some(span));
        assert_eq!(
            diagnostic.related,
            [Related {
                span,
                message: "used here".into()
            }]
        );
        #[cfg(feature = "diagnostic-fixes")]
        {
            assert_eq!(called.get(), 2);
            assert_eq!(diagnostic.fixes[0].edits[0].replacement, "total");
            assert!(!diagnostic.fixes[0].safe);
            assert_eq!(diagnostic.fixes[1].edits[0].replacement, "");
            assert!(diagnostic.fixes[1].safe);
        }
        #[cfg(not(feature = "diagnostic-fixes"))]
        assert_eq!(called.get(), 0);
    }
}
