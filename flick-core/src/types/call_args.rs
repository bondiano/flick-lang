//! One argument of an application checked against its parameter, and
//! the arguments an application checks after the others.

use std::mem;

use super::Checker;
use super::sugar_lints::unparen;
use super::ty::{Stage, Type};
use crate::consteval;
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The accessors `late` among `args`, left until the other arguments
    /// of `head` gave the domain of their parameter: in
    /// `List.map .address.town users` a missing field is at its step.
    pub(super) fn check_late(&mut self, head: &Node, args: &[&Node], late: Vec<(usize, Type)>) {
        for (i, param) in late {
            let since = self.diagnostics.len();
            self.check_value(args[i], &param);
            self.explain_argument(since, head, i, &param);
        }
    }

    /// Whether an accessor given for `param` waits for the other arguments:
    /// `param` is a pure arrow whose domain nothing has fixed yet. An
    /// accessor holds nothing and runs nothing, so checking it later changes
    /// no effect and no region.
    pub(super) fn late_accessor(&self, arg: &Node, param: &Type) -> bool {
        unparen(arg).kind == K::AccessorExpr
            && matches!(
                self.whnf(param),
                Type::Fun { param: domain, effects: None, .. }
                    if matches!(self.whnf(&domain), Type::Meta(_))
            )
    }

    /// The argument `arg`, the `i`-th of `head`, checked against `param`;
    /// `given` collects it for a watched name.
    pub(super) fn argument(
        &mut self,
        head: &Node,
        (arg, i): (&Node, usize),
        param: &Type,
        defaults: &mut Vec<(Type, usize)>,
        given: &mut Vec<super::facts::Given>,
        (watched, marked): (bool, &[usize]),
    ) -> Type {
        let calls = self.cx.stage.calls;
        let since = self.diagnostics.len();
        // a task acquiring into a region a later argument names
        // waits for it (`docs/formal/regions.md#scope-region-of-a-task-argument-is-resolved-after-the-call`)
        let eff = self.eff_parts(param).is_some();
        let defer = eff || self.arrow_stages(param).contains(&Stage::Runtime);
        let outer = defer.then(|| self.cx.regions.defaults.replace(Vec::new()));
        // a runtime call for a parameter marked `~` is passed unstarted
        let argument = self.meeting_parameter(head, i, param, |c| {
            if eff && marked.contains(&i) && c.runs_now(arg) {
                c.suspended_arg(arg, param)
            } else {
                c.check_value(arg, param)
            }
        });
        if let Some(outer) = outer {
            let mine = mem::replace(&mut self.cx.regions.defaults, outer);
            defaults.extend(mine.unwrap_or_default());
        }
        self.explain_argument(since, head, i, param);
        if watched {
            let folded = consteval::eval(arg, self.text);
            let lambda = unparen(arg).kind == K::FunExpr;
            given.push((self.span(arg), argument.clone(), folded, lambda));
        }
        if eff {
            if self.cx.stage.calls > calls {
                self.early_argument(head, arg);
            }
            // the rows the `Eff` needs are lower bounds of the parameter's: grown
            // now, before the result's rows unify as if they could be smaller
            self.grow();
        }
        argument
    }

    /// A runtime call in an argument runs before the function it is passed
    /// to; a function that takes an `Eff` wants the computation unstarted.
    fn early_argument(&mut self, head: &Node, arg: &Node) {
        let span = self.span(arg);
        let name = self.text_of(head);
        self.diagnostics.push(
            Diagnostic::new(
                codes::E003,
                Severity::Warning,
                format!(
                    "this argument runs a runtime call before `{name}` is applied, although `{name}` takes an `Eff` to run itself"
                ),
            )
            .with_span(span).with_fix(|| ("pass the computation unstarted with `task { … }`", vec![
                Edit {
                    span: Span::new(span.file, span.start, span.start),
                    replacement: "task { ".to_owned(),
                },
                Edit {
                    span: Span::new(span.file, span.end, span.end),
                    replacement: " }".to_owned(),
                },
            ])),
        );
    }
}

/// A function written where it is applied, whose argument gives the type
/// of its first parameter: an accessor, or a `fun` whose first parameter is
/// a bare name or `_` (a pattern or an annotation keeps its own type).
pub(super) fn applied_where_written(head: &Node) -> bool {
    let head = unparen(head);
    match head.kind {
        K::AccessorExpr => true,
        K::FunExpr => head
            .nodes()
            .find(|n| super::patterns::is_pattern(n.kind))
            .is_some_and(|p| matches!(p.kind, K::VarPat | K::WildcardPat)),
        _ => false,
    }
}

#[cfg(test)]
#[path = "call_args_tests.rs"]
mod tests;
