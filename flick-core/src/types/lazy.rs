//! `lazy { … }` and `lazy e`: the task of the body, kept to run at most once,
//! at its first `await` (`docs/formal/lazy.md` §2). Building one allocates
//! a cell, so it is a runtime call, as `Ref.make` is.

use super::Checker;
use super::infer::eff;
use super::lang::Lang;
use super::ty::{Effects, Type};
use crate::syntax::cst::Node;

/// `Lazy services errors result`.
pub fn lazy(services: Type, errors: Type, result: Type) -> Type {
    Type::apps(Type::lang(Lang::Lazy), [services, errors, result])
}

impl Checker<'_> {
    pub fn lazy_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let expected_task = expected.and_then(|t| {
            let (services, errors, result) = self.lazy_parts(t)?;
            let task = eff(services, errors, result);
            Some(match self.hidden_qualifier(t) {
                Some(captures) => Self::qualify_hidden(task, captures),
                None => task,
            })
        });
        let task = self.task(node, expected_task.as_ref());
        let Some((services, errors, result)) = self.eff_parts(&task) else {
            return Type::Error;
        };
        let span = self.span(node);
        // its initializer runs synchronously in the forcing fiber, so `abort`
        // can unwind to the forcer's handler, while `control` cannot split it
        self.want(
            Type::app(Type::lang(Lang::NoControl), services.clone()),
            span,
        );
        let cell = Effects::new(Type::empty_row(), Type::empty_row());
        self.call_effects(&cell, span);
        lazy(services, errors, result)
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
