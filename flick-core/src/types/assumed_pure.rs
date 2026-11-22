//! A local applied before its type is known is taken for a pure function,
//! `(fun h -> h ()) f`. When a runtime function then meets that arrow, the
//! mismatch points at the local and offers the annotation it needs.

use std::mem;

use super::Checker;
use super::bindings::LetParts;
use super::sugar_lints::unparen;
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

    /// Notes where the parameters of the local function bound at `at` are
    /// bound, a `fun` right after its `=` continuing them.
    pub(super) fn note_local_params(&mut self, at: u32, parts: &LetParts) {
        let mut params = parts.params.clone();
        if let Some(body) = parts.body {
            let mut statements = body.nodes();
            if let (Some(lambda), None) = (statements.next(), statements.next())
                && lambda.kind == K::FunExpr
            {
                params.extend(
                    lambda
                        .nodes()
                        .filter(|n| super::patterns::is_pattern(n.kind)),
                );
            }
        }
        let binders = params.into_iter().map(binder).collect();
        self.cx.local_params.insert(at, binders);
    }

    /// Checks an argument of `head` with `check`: when it is the `i`th of a
    /// local function or a `fun`, a mismatch with `param` is explained at
    /// that function's parameter.
    pub(super) fn meeting_parameter<T>(
        &mut self,
        head: &Node,
        i: usize,
        param: &Type,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let parameter = self.parameter_binder(head, i).map(|at| (at, param.clone()));
        let outer = mem::replace(&mut self.cx.parameter, parameter);
        let checked = check(self);
        self.cx.parameter = outer;
        checked
    }

    /// Where the `i`th parameter of `head` is bound, when `head` is a local
    /// function without a signature or a `fun`.
    fn parameter_binder(&self, head: &Node, i: usize) -> Option<u32> {
        let head = unparen(head);
        match head.kind {
            K::FunExpr => (head.nodes())
                .filter(|n| super::patterns::is_pattern(n.kind))
                .nth(i)
                .and_then(binder),
            K::NameExpr => {
                let mut tokens = head.child_tokens().filter(|t| t.kind != K::Dot);
                let (Some(token), None) = (tokens.next(), tokens.next()) else {
                    return None;
                };
                let (name, generated) = (token.text(self.text), token.start >= self.generated);
                let local = (self.cx.locals.iter())
                    .rfind(|l| l.name == name && l.generated == generated)?;
                *self.cx.local_params.get(&local.at?)?.get(i)?
            }
            _ => None,
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
        // the parameter the argument meets, else the arrow expected itself
        let parameter = (self.cx.parameter.as_ref())
            .filter(|(_, ty)| super::report::uncaptured(&self.zonk(ty)) == *expected)
            .and_then(|(binder, _)| {
                (self.cx.assumed_pure.iter()).position(|(_, _, at)| at == binder)
            });
        let found = parameter.or_else(|| {
            (self.cx.assumed_pure.iter())
                .position(|(arrow, _, _)| super::report::uncaptured(&self.zonk(arrow)) == *expected)
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

/// Where the parameter `pattern` binds its name, when it is one.
fn binder(pattern: &Node) -> Option<u32> {
    let pattern = match pattern.kind {
        K::ParenPat => pattern.nodes().next()?,
        _ => pattern,
    };
    (pattern.kind == K::VarPat)
        .then(|| pattern.child_tokens().find(|t| t.kind == K::Ident))?
        .map(|t| t.start)
}

#[cfg(test)]
#[path = "assumed_pure_tests.rs"]
mod tests;
