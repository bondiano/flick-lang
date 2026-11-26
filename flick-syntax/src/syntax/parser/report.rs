//! Reporting what the parser did not expect: one error per statement.

use std::iter;

use super::{Ctx, K, Parser};
use crate::diagnostics::{Code, Diagnostic, codes};
use crate::source::Span;

impl Parser<'_> {
    pub(super) fn found(&self) -> String {
        let t = self.raw();
        if t.kind == K::Eof {
            "end of file".into()
        } else if self.sep().is_some() {
            "the end of the line".into()
        } else {
            format!("`{}`", self.text_of(t))
        }
    }

    pub(super) fn error_at(&mut self, code: Code, span: Span, message: String) {
        self.report(Diagnostic::error(code, message, span));
    }

    /// Reports `diagnostic`, unless an error was already reported in this
    /// statement or at this token.
    pub(super) fn report(&mut self, diagnostic: Diagnostic) {
        if self.failed || self.last_error_at == Some(self.pos) {
            return;
        }
        self.failed = true;
        self.last_error_at = Some(self.pos);
        self.errors.push(diagnostic);
    }

    pub(super) fn error_expected(&mut self, what: &str) {
        let message = format!("expected {what}, found {}", self.found());
        self.error_at(codes::S007, self.here_span(), message);
    }

    /// Reports an unexpected token after a complete statement, as an
    /// indentation problem when it starts a line.
    pub(super) fn unexpected(&mut self) {
        self.failed = false;
        self.report_unexpected();
    }

    pub(super) fn report_unexpected(&mut self) {
        let t = self.raw();
        if t.line_start {
            let cols: Vec<String> = self
                .ctx
                .iter()
                .filter_map(|c| match c.0 {
                    Ctx::Aligned { col } => Some((col + 1).to_string()),
                    _ => None,
                })
                .collect();
            let message = format!(
                "unexpected indentation: column {} continues nothing here; \
                 statements of the enclosing blocks start at column {}",
                t.col + 1,
                iter::once("1".to_owned())
                    .chain(cols)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            self.error_at(codes::S009, self.span(t.start, t.end), message);
        } else {
            let message = format!("unexpected {}", self.found());
            self.error_at(codes::S007, self.here_span(), message);
        }
    }
}
