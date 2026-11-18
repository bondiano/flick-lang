//! `lazy { … }` and `lazy e`: the task of the body, kept to run at most once,
//! at its first `await` (`docs/formal/lazy.md` §2). Building one allocates
//! a cell, so it is a runtime call, as `Ref.make` is; forcing it may wait
//! for the fiber running it, so `await` of a `Lazy` needs `Async`.

use super::Checker;
use super::infer::eff;
use super::lang::{self, Lang};
use super::ty::{Effects, Row, Type};
use crate::syntax::cst::Node;
use crate::types::Elem;

/// `Lazy services errors result`.
pub fn lazy(services: Type, errors: Type, result: Type) -> Type {
    Type::apps(Type::lang(Lang::Lazy), [services, errors, result])
}

impl Checker<'_> {
    pub fn lazy_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let expected_task = (expected.and_then(|t| self.lazy_parts(t)))
            .map(|(services, errors, result)| eff(services, errors, result));
        let task = self.task(node, expected_task.as_ref());
        let Some((services, errors, result)) = self.eff_parts(&task) else {
            return Type::Error;
        };
        let span = self.span(node);
        // its first force may come from a fiber other than the one that built it
        self.want(
            Type::app(Type::lang(Lang::Detached), services.clone()),
            span,
        );
        let cell = Effects::new(Type::empty_row(), Type::empty_row());
        self.call_effects(&cell, span);
        lazy(services, errors, result)
    }

    /// `await` of `ty`, at `node`, when `ty` is a `Lazy`: its result. Forcing
    /// runs the initializer here, or waits for the fiber running it.
    pub fn force(&mut self, node: &Node, ty: &Type) -> Option<Type> {
        let (services, errors, result) = self.lazy_parts(ty)?;
        self.typing.forced.insert(node.range());
        let span = self.span(node);
        // a `let lazy` is its function's alone: nothing else can be forcing it
        let shared = !node
            .nodes()
            .next()
            .is_some_and(|operand| self.lazy_local(operand));
        if !shared {
            self.typing.read_lazy.insert(node.range());
        }
        if self.runtime_allowed(span) && shared {
            let label = Elem::named(&lang::item(Lang::Async).label);
            let waits = Type::Row(Row::closed(vec![(label, Type::lang(Lang::Async))]));
            self.perform(&waits, &Type::empty_row(), span);
        }
        if self.runtime_allowed(span) {
            self.perform(&services, &errors, span);
        }
        Some(result)
    }

    /// The rows and result of a `Lazy` type.
    pub fn lazy_parts(&self, t: &Type) -> Option<(Type, Type, Type)> {
        let t = self.whnf(t);
        let (head, args) = t.spine();
        match args[..] {
            [r, e, a] if head.is_lang(Lang::Lazy) => Some((r.clone(), e.clone(), a.clone())),
            _ => None,
        }
    }
}
