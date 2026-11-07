//! `todo` and `echo`: debugging aids that check anywhere and warn.

use std::mem;

use super::Checker;
use super::kinds::Kind;
use super::ty::Type;
use super::typing::range;
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::{Node, Token};
use crate::types::lang::Lang;

impl Checker<'_> {
    /// `todo`: any type its place needs.
    pub(super) fn todo(&mut self, node: &Node) -> Type {
        self.warning(
            codes::T022,
            self.span(node),
            "`todo` is left in the code: running it fails",
        );
        // the message names the place of the `todo`
        self.want(Type::lang(Lang::CallSite), self.span(node));
        self.fresh()
    }

    /// `echo e` has the type of `e`; a bare `echo` is the identity function.
    pub(super) fn echo(&mut self, node: &Node, op: Token, operand: Option<&Node>) -> Type {
        let warning = Diagnostic::new(codes::T023, Severity::Warning, "`echo` is left in the code")
            .with_span(self.token_span(op));
        self.want(Type::lang(Lang::CallSite), self.token_span(op));
        let Some(operand) = operand else {
            self.diagnostics.push(warning);
            let a = self.fresh();
            return Type::pure_fun(a.clone(), a);
        };
        self.diagnostics.push(warning.with_fix(|| {
            (
                "remove `echo`",
                vec![Edit {
                    span: Span::new(self.file, op.start, operand.range().0),
                    replacement: String::new(),
                }],
            )
        }));
        let ty = self.infer(operand);
        self.note_type(self.span(node), &ty);
        ty
    }

    /// The variable for a `_` in a type, of the kind its place expects; an
    /// error type after the error for a `_` outside a signature or annotation.
    pub(super) fn hole(&mut self, span: Span, expected: &Kind) -> (Type, Kind) {
        if !self.hole_ok {
            self.error(
                codes::T024,
                span,
                "`_` stands for a type only in a signature or an annotation",
            );
            return (Type::Error, expected.clone());
        }
        let hole = self.fresh();
        self.holes.push((span, hole.clone(), false));
        (hole, expected.clone())
    }

    /// A type placeholder is inferred like `_`, but must be replaced before build.
    pub(super) fn todo_type(&mut self, span: Span, expected: &Kind) -> (Type, Kind) {
        let hole = self.fresh();
        self.holes.push((span, hole.clone(), true));
        (hole, expected.clone())
    }

    /// A diagnostic for each type placeholder, with its inferred type; one
    /// of generated code is a fact for its plugin's `check` too.
    pub(super) fn report_holes(&mut self) {
        let clean = !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
        for (span, hole, required) in mem::take(&mut self.holes) {
            if span.start >= self.generated {
                let fact = self.fact(&hole);
                self.typing.wildcards.push((range(span), fact));
            }
            let shown = self.show(&hole);
            let (severity, message) = if required {
                (
                    Severity::Error,
                    format!("replace `todo` with its inferred type `{shown}`"),
                )
            } else {
                (Severity::Warning, format!("`_` is `{shown}`"))
            };
            let complete = !shown
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|w| w == "_");
            self.diagnostics.push(
                Diagnostic::new(codes::T024, severity, message)
                    .with_span(span)
                    .with_fix(|| {
                        if required && (!complete || !clean) {
                            return (format!("write `{shown}`"), Vec::new());
                        }
                        let bare = !shown.contains(' ') || shown.starts_with(['(', '{', '[']);
                        let replacement = if bare {
                            shown.clone()
                        } else {
                            format!("({shown})")
                        };
                        (format!("write `{shown}`"), vec![Edit { span, replacement }])
                    }),
            );
        }
    }
}
