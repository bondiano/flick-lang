//! `lazy { … }` and `await` of a `Lazy` (`docs/formal/lazy.md`): calls of
//! the Prelude's `lazy_make` and `lazy_force`, which the runtimes implement;
//! and the tasks of arguments passed unstarted to a parameter marked `~`.

use super::Lower;
use crate::ir::{Expr, ExprKind, ExprLang, Lit, Stage};
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

    /// The value of `let lazy name = body`: the cell of the task of `body`.
    pub(super) fn lazy_let(&mut self, decl: &Node, body: Option<&Node>) -> Expr {
        let span = self.span(decl);
        match body {
            Some(body) => Expr::new(self.lazy_expr(decl, body, span), span),
            None => Expr::new(ExprKind::Lit(Lit::Unit), span),
        }
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

/// A value assigned to a `let lazy mut`: a cell already done with it.
pub(super) fn done(value: Expr, span: Span) -> Expr {
    Expr::new(runtime_call(Lang::LazyDone, value, span), span)
}

/// `await` of a `Lazy`: `lazy_force` of it, or `lazy_read` of a `let lazy`.
pub(super) fn force(operand: Expr, span: Span, local: bool) -> ExprKind {
    let read = if local {
        Lang::LazyRead
    } else {
        Lang::LazyForce
    };
    runtime_call(read, operand, span)
}

fn runtime_call(function: Lang, arg: Expr, span: Span) -> ExprKind {
    ExprKind::App {
        func: Box::new(Expr::new(ExprKind::lang(function), span)),
        arg: Box::new(arg),
        stage: Stage::Runtime,
        arg_first: false,
    }
}
