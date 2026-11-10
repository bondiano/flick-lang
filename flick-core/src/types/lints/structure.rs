//! T036–T039: a function too long, nested too deep, with too many branches
//! or parameters, past the `[lint]` thresholds. U007: a function that reads
//! atoms to build a `Ui`, which a `component` does with its own re-render.

use super::super::Checker;
use super::super::let_parts;
use super::super::ty::Type;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::Span;
use crate::syntax::{cst::Node, structure};
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// The lints of a function `decl`, a `let` with parameters; other
    /// declarations have none.
    pub(super) fn structure(&mut self, decl: &Node) {
        let parts = let_parts(decl, self.text);
        let body = (!parts.params.is_empty()).then_some(parts.body).flatten();
        let (Some(body), Some(pattern)) = (body, parts.pattern) else {
            return;
        };
        let name = parts.name.unwrap_or_default();
        let at = self.span(pattern);
        let limits = self.options.thresholds;
        let warn =
            |code, message: String| Diagnostic::new(code, Severity::Warning, message).with_span(at);

        let structure = structure::measure(body, self.text, limits);
        let lines = structure.lines;
        if lines > limits.function_lines {
            self.diagnostics.push(
                warn(
                    codes::T036,
                    format!(
                        "`{name}` is {lines} lines long, more than {}",
                        limits.function_lines
                    ),
                )
                .with_fix(|| ("split it into smaller functions", Vec::new())),
            );
        }
        let complexity = structure.complexity;
        if complexity > limits.complexity {
            self.diagnostics.push(
                warn(
                    codes::T038,
                    format!(
                        "`{name}` has a complexity of {complexity}, more than {}",
                        limits.complexity
                    ),
                )
                .with_fix(|| {
                    (
                        "move some of its branches into functions of their own",
                        Vec::new(),
                    )
                }),
            );
        }
        let params = parts.params.len();
        if params > limits.params as usize {
            self.diagnostics.push(
                warn(
                    codes::T039,
                    format!(
                        "`{name}` takes {params} parameters, more than {}",
                        limits.params
                    ),
                )
                .with_fix(|| ("take a record of them", Vec::new())),
            );
        }
        for nesting in structure.nesting {
            self.diagnostics.push(
                Diagnostic::new(
                    codes::T037,
                    Severity::Warning,
                    format!(
                        "nested {} levels deep, more than {}",
                        nesting.depth, limits.nesting
                    ),
                )
                .with_span(Span::new(self.file, nesting.span.start, nesting.span.end))
                .with_fix(|| ("move the inner part into a function of its own", Vec::new())),
            );
        }
        self.reads_atoms(&name, at);
    }

    /// U007: a top-level function whose result is a `Ui` it builds from
    /// atoms it reads.
    fn reads_atoms(&mut self, name: &str, at: Span) {
        let Some(mut ty) = self.signed_type(name) else {
            return;
        };
        if let Type::Forall { body, .. } = ty {
            ty = *body;
        }
        let mut reads = false;
        while let Type::Fun {
            effects, result, ..
        } = ty
        {
            if let Some(effects) = effects
                && let Type::Row(row) = &effects.services
            {
                reads |= (row.entries.iter())
                    .any(|(l, _)| Some(l.as_str()) == lang::label(Lang::AtomRead));
            }
            ty = *result;
        }
        if let Type::Captured { ty: inner, .. } = ty {
            ty = *inner;
        }
        if reads && ty.is_lang(Lang::Ui) {
            self.diagnostics.push(
                Diagnostic::new(
                    codes::U007,
                    Severity::Warning,
                    format!("`{name}` reads atoms to build a `Ui`: whatever calls it renders again when they change"),
                )
                .with_span(at).with_fix(|| ("make it a `component`: it renders again on its own, not with its parent", Vec::new())),
            );
        }
    }
}
