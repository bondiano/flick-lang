//! T036–T039: a function too long, nested too deep, with too many branches
//! or parameters, past the `[lint]` thresholds; and the functions whose
//! signatures name a service a plugin watches, which that plugin is told of.

use super::super::Checker;
use super::super::let_parts;
use super::super::ty::Type;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::Span;
use crate::syntax::{cst::Node, structure};
use flick_plugin::{Binding, CallSite};

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
        self.named_in_signature(&name, at, decl);
    }

    /// The services a plugin watches that the signature of the top-level
    /// function `name` names: each is a place its plugin is told of, at the
    /// function's name, with what the function gives at last.
    fn named_in_signature(&mut self, name: &str, at: Span, decl: &Node) {
        let Some(signed) = self.signed_type(name) else {
            return;
        };
        let (mut ty, constrained) = match &signed {
            Type::Forall {
                body, constraints, ..
            } => (&**body, !constraints.is_empty()),
            other => (other, false),
        };
        let mut found: Vec<(&str, &str)> = Vec::new();
        while let Type::Fun {
            effects, result, ..
        } = ty
        {
            if let Some(effects) = effects
                && let Type::Row(row) = &effects.services
            {
                let named = row.entries.iter().filter_map(|(l, _)| l.name());
                for service in named.filter_map(|n| n.rsplit_once('.')) {
                    if self.options.watched.holds(service.0, service.1) && !found.contains(&service)
                    {
                        found.push(service);
                    }
                }
            }
            ty = result;
        }
        if let Type::Captured { ty: inner, .. } = ty {
            ty = inner;
        }
        let span = |(start, end)| flick_plugin::Span { start, end };
        for (module, service) in found {
            self.typing.call_sites.push(CallSite {
                module: module.to_owned(),
                name: service.to_owned(),
                span: span((at.start, at.end)),
                arguments: Vec::new(),
                refinement: None,
                generated: at.start >= self.generated,
                binding: Some(Binding {
                    module: self.env.module.clone(),
                    name: Some(name.to_owned()),
                    span: span(decl.range()),
                    constrained,
                    in_function: false,
                    applied: 0,
                    gives: Some(self.fact_type(ty)),
                }),
            });
        }
    }
}
