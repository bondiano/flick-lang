//! `let lazy x = e` and `let lazy mut x = e` (`docs/formal/lazy.md` §4): a
//! binding of its function, as `let mut` is, forced with `await x` at most
//! once. Nothing but its own function reaches it, so no other fiber can be
//! forcing it: `await x` needs no `Async`. A `let lazy mut` written before
//! it is read never runs `e`.

use super::Checker;
use super::bindings::LetParts;
use super::lazy::lazy;
use super::mutation::Binding;
use super::ty::{Effects, Type};
use super::typing::Ref;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// `let lazy [mut] name = body`: the task of `body`, as the type
    /// `Lazy r e a` of a binding that only `await` reads.
    pub(super) fn local_lazy(&mut self, decl: &Node, parts: &LetParts, name: &str, at: u32) {
        let ty = match parts.body {
            Some(body) => {
                let task = self.deferred(decl, body, None);
                match self.eff_parts(&task) {
                    Some((services, errors, result)) => {
                        if let Some(header) = parts.header {
                            let written = self.annotation(header);
                            self.expect(&result, &written, self.span(header));
                        }
                        lazy(services, errors, result)
                    }
                    None => Type::Error,
                }
            }
            None => Type::Error,
        };
        // building it is a runtime call, as `lazy { … }` is
        let cell = Effects::new(Type::empty_row(), Type::empty_row());
        self.call_effects(&cell, self.span(decl));
        self.typing.lazy_lets.insert(decl.range());
        self.push_local(name, at, ty);
        if let Some(local) = self.cx.locals.last_mut() {
            local.binding = if decl.has_token(K::MutKw) {
                Binding::LazyMut
            } else {
                Binding::Lazy
            };
        }
    }

    /// Whether `operand` names a `let lazy` binding.
    pub(super) fn lazy_local(&self, operand: &Node) -> bool {
        if operand.kind != K::NameExpr
            || !matches!(self.typing.names.get(&operand.range()), Some(Ref::Local))
        {
            return false;
        }
        let name = self.text_of(operand);
        (self.cx.locals.iter().rev())
            .find(|l| l.name == name)
            .is_some_and(|l| matches!(l.binding, Binding::Lazy | Binding::LazyMut))
    }

    /// Whether the use of the local `index` is one of a `let lazy` other
    /// than as `await name`, which it reports.
    pub(super) fn lazy_unforced(&mut self, index: usize, span: Span) -> bool {
        let local = &self.cx.locals[index];
        if !matches!(local.binding, Binding::Lazy | Binding::LazyMut) || self.cx.awaiting {
            return false;
        }
        let name = local.name.clone();
        self.error(
            codes::T003,
            span,
            format!("`{name}` is a `let lazy`, read only as `await {name}`: share a value with `lazy {{ … }}`"),
        );
        true
    }

    /// The type `value` must have to be assigned to the `let lazy mut` local
    /// `index`: the result of its `Lazy`, for the assignment at `node`.
    pub(super) fn lazy_assigned(&mut self, index: usize, node: &Node) -> Option<Type> {
        if self.cx.locals[index].binding != Binding::LazyMut {
            return None;
        }
        self.typing.lazy_sets.insert(node.range());
        let ty = self.cx.locals[index].ty.clone();
        Some(
            self.lazy_parts(&ty)
                .map_or(Type::Error, |(_, _, result)| result),
        )
    }
}
