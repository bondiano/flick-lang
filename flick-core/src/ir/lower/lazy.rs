//! `lazy { … }` and `await` of a `Lazy` (`docs/formal/lazy.md`): calls of
//! the Prelude's `lazy_make` and `lazy_force`, which the runtimes implement;
//! and the tasks of arguments passed unstarted to a parameter marked `~`.

use super::Lower;
use crate::ir::{Expr, ExprKind, ExprLang, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::StageKey;
use crate::types::lang::Lang;

impl Lower<'_> {
    /// An argument, or the task of one passed unstarted to a parameter
    /// marked `~` (`types::suspended`).
    pub(super) fn argument(&mut self, arg: &Node) -> Expr {
        if !self.typing.suspends.contains(&arg.range()) {
            return self.expr(arg);
        }
        let span = self.span(arg);
        let key = StageKey::Task { node: arg.range() };
        self.register_owner_entry(key);
        let body = self.expr(arg);
        Expr::new(ExprKind::Task(Box::new(self.owner_entry(key, body))), span)
    }

    /// `lazy body`: the task of `body`, kept in a cell by `lazy_make`.
    pub(super) fn lazy_expr(&mut self, node: &Node, body: &Node, span: Span) -> ExprKind {
        let key = StageKey::Task { node: node.range() };
        self.register_owner_entry(key);
        let body = self.expr(body);
        let task = Expr::new(ExprKind::Task(Box::new(self.owner_entry(key, body))), span);
        runtime_call(Lang::LazyMake, task, span)
    }
}

/// `await` of a `Lazy`: `lazy_force` of it.
pub(super) fn force(operand: Expr, span: Span) -> ExprKind {
    runtime_call(Lang::LazyForce, operand, span)
}

fn runtime_call(function: Lang, arg: Expr, span: Span) -> ExprKind {
    ExprKind::App {
        func: Box::new(Expr::new(ExprKind::lang(function), span)),
        arg: Box::new(arg),
        stage: Stage::Runtime,
        arg_first: false,
    }
}
