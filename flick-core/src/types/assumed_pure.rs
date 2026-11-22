//! A local applied before its type is known is taken for a pure function,
//! `let run = fun h -> h ()` / `run f`. When a runtime function then meets
//! that arrow, the mismatch points at the local and offers the annotation it
//! needs.

use std::{iter, mem};

use super::Checker;
use super::bindings::LetParts;
use super::sugar_lints::unparen;
use super::ty::Type;
use crate::diagnostics::{Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

pub(super) struct Assumption {
    arrow: Type,
    name: String,
    at: u32,
    /// A diagnostic already explained this assumption. Later diagnostics
    /// can reuse it when they know the binder they are checking.
    reported: bool,
}

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
            self.cx.assumed_pure.push(Assumption {
                arrow: arrow.clone(),
                name: name.to_owned(),
                at,
                reported: false,
            });
        }
    }

    /// Notes where the stages of a local function bind their parameters.
    pub(super) fn note_local_params(&mut self, at: u32, parts: &LetParts) {
        let lambda = parts.body.and_then(only_lambda);
        if parts.params.is_empty() && lambda.is_none() {
            return;
        }
        let binders = parts
            .params
            .iter()
            .copied()
            .chain(lambda.into_iter().flat_map(lambda_params))
            .map(binder)
            .collect();
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
    fn parameter_binder(&self, mut head: &Node, mut i: usize) -> Option<u32> {
        loop {
            head = unparen(head);
            if head.kind != K::AppExpr {
                break;
            }
            let mut parts = head.nodes();
            head = parts.next()?;
            for argument in parts {
                if argument.kind == K::Children {
                    return None;
                }
                i += 1;
            }
        }
        match head.kind {
            K::FunExpr => lambda_params(head).nth(i).and_then(binder),
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
        if !self
            .diagnostics
            .last()
            .is_some_and(|d| d.code == codes::T003 && d.span == Some(span))
        {
            return;
        }
        let parameter = (self.cx.parameter.as_ref())
            .filter(|(_, ty)| super::report::uncaptured(&self.zonk(ty)) == *expected);
        let found = if let Some((binder, _)) = parameter {
            // Every use of this parameter can point to the same binding.
            self.cx
                .assumed_pure
                .iter()
                .position(|assumption| assumption.at == *binder)
        } else {
            self.arrow_without_parameter(expected)
        };
        let Some(index) = found else {
            return;
        };
        let assumption = &mut self.cx.assumed_pure[index];
        assumption.reported = true;
        let (name, at) = (assumption.name.clone(), assumption.at);
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
        let note =
            format!("`{name}` has no type written, so applying it took it for a pure function");
        let mut last = last.with_related(binder, note);
        // A let header takes its parameter's type in the signature (T005).
        if !self.root.is_some_and(|root| let_parameter(root, at)) {
            last = last.with_fix(|| {
                let shown = self.show_written(actual);
                (
                    format!("annotate `{name}` with the runtime function it is given"),
                    vec![Edit {
                        span: binder,
                        replacement: format!("({name} : {shown})"),
                    }],
                )
            });
        }
        self.diagnostics.push(last);
    }

    /// Uses arrow shape when the callee has no source parameter and one
    /// assumption matches. A previously explained assumption can only
    /// be reused through its source binder.
    fn arrow_without_parameter(&self, expected: &Type) -> Option<usize> {
        let mut matches = self
            .cx
            .assumed_pure
            .iter()
            .enumerate()
            .filter(|(_, assumption)| {
                !assumption.reported
                    && super::report::uncaptured(&self.zonk(&assumption.arrow)) == *expected
            });
        let (index, _) = matches.next()?;
        matches.next().is_none().then_some(index)
    }
}

/// A function body consisting of a lambda, with optional parentheses.
fn only_lambda(body: &Node) -> Option<&Node> {
    let mut statements = body.nodes();
    let node = unparen(statements.next()?);
    (node.kind == K::FunExpr && statements.next().is_none()).then_some(node)
}

/// The parameters of a lambda and of each lambda it immediately returns.
fn lambda_params(lambda: &Node) -> impl Iterator<Item = &Node> {
    iter::successors(Some(lambda), |node| {
        node.nodes()
            .find(|child| child.kind == K::Block)
            .and_then(only_lambda)
    })
    .flat_map(|node| node.nodes().filter(|n| super::patterns::is_pattern(n.kind)))
}

/// Whether the binder at `at` belongs to a named function's let header.
fn let_parameter(mut node: &Node, at: u32) -> bool {
    while let Some(child) = node.nodes().find(|child| {
        let (start, end) = child.range();
        start <= at && at < end
    }) {
        if node.kind == K::LetDecl && child.kind == K::Params {
            return true;
        }
        node = child;
    }
    false
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
