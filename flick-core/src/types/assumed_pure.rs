//! A local applied before its type is known is taken for a pure function,
//! `(fun h -> h ()) f`. When a runtime function then meets that arrow, the
//! mismatch points at the local and offers the annotation it needs.

use std::collections::HashMap;

use super::Checker;
use super::ty::Type;
use crate::diagnostics::{Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Notes that `head`, applied with no type known, was taken for `arrow`.
    pub(super) fn assume_pure(&mut self, head: &Node, arrow: &Type) {
        if head.kind != K::NameExpr {
            return;
        }
        let (start, end) = head.range();
        let name = &self.text[start as usize..end as usize];
        let found = self.cx.locals.iter().rposition(|l| l.name == name);
        if let Some(at) = found.and_then(|i| self.cx.locals[i].at) {
            self.cx
                .assumed_pure
                .push((arrow.clone(), name.to_owned(), at));
        }
    }

    /// When `expected`, a pure arrow taken for a local, met the runtime
    /// function `actual`: the `FLICK-T003` just reported at `span` says so at
    /// the local's binder and offers to annotate it.
    pub(super) fn explain_assumed_pure(
        &mut self,
        actual: &Type,
        expected: &Type,
        source_types: (&Type, &Type),
        span: Span,
    ) {
        let runtime = matches!(
            actual,
            Type::Fun {
                effects: Some(_),
                ..
            }
        );
        if !runtime || !matches!(expected, Type::Fun { effects: None, .. }) {
            return;
        }
        let reported = (self.diagnostics.last())
            .is_some_and(|d| d.code == codes::T003 && d.span == Some(span));
        let found = (self.cx.assumed_pure.iter()).position(|(arrow, _, _)| {
            let taken = super::report::uncaptured(&self.zonk(arrow));
            instance_of(&taken, expected, &mut HashMap::new())
        });
        let (true, Some(index)) = (reported, found) else {
            return;
        };
        // one explanation per arrow taken
        let (_, name, at) = self.cx.assumed_pure.remove(index);
        let mut last = self.diagnostics.pop().expect("the T003 just reported");
        let end = at + u32::try_from(name.len()).expect("source fits in u32");
        let binder = Span::new(self.file, at, end);
        let show_side = |t: &Type| {
            let plain = super::report::uncaptured(&self.zonk(t));
            if matches!(plain, Type::Forall { .. }) {
                self.show_exact_written(&plain)
            } else {
                self.show_written(&plain)
            }
        };
        let (found_msg, expected_msg) = (show_side(source_types.0), show_side(source_types.1));
        last.message = format!("expected `{expected_msg}`, found `{found_msg}`");
        let shown = self.show_written(actual);
        let annotate = Edit {
            span: binder,
            replacement: format!("({name} : {shown})"),
        };
        let note =
            format!("`{name}` has no type written, so applying it took it for a pure function");
        let last = last.with_related(binder, note).with_fix(|| {
            (
                format!("annotate `{name}` with the runtime function it is given"),
                vec![annotate],
            )
        });
        self.diagnostics.push(last);
    }
}

/// Whether `ty` is `taken` with its variables replaced: the arrow taken for
/// a local of a function generalized since, met at one of its uses.
fn instance_of<'t>(taken: &'t Type, ty: &'t Type, vars: &mut HashMap<&'t str, &'t Type>) -> bool {
    match (taken, ty) {
        (Type::Var(name), _) => *vars.entry(name).or_insert(ty) == ty,
        (
            Type::Fun {
                param: p,
                effects: e,
                result: r,
            },
            Type::Fun {
                param,
                effects,
                result,
            },
        ) => e == effects && instance_of(p, param, vars) && instance_of(r, result, vars),
        (Type::App { head: h, arg: a }, Type::App { head, arg }) => {
            instance_of(h, head, vars) && instance_of(a, arg, vars)
        }
        (Type::Tuple(ts), Type::Tuple(us)) => {
            ts.len() == us.len() && ts.iter().zip(us).all(|(t, u)| instance_of(t, u, vars))
        }
        _ => taken == ty,
    }
}

#[cfg(test)]
#[path = "assumed_pure_tests.rs"]
mod tests;
