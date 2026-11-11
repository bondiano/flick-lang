//! Strict list comprehensions use the existing core list recursion.
//! Each iteration keeps its local SSA bindings; outer assignments are rejected.
use std::rc::Rc;

use super::{
    Tr,
    expr::pat,
    loops::{effectful, list_step},
};
use crate::syntax::{Arm, Pat, Prim, Term};
use flick_core::ir::{self, ExprKind};

/// Yield becomes a singleton list; a nested clause starts another collection.
fn collected(e: &ir::Expr) -> ir::Expr {
    let kind = match &e.kind {
        ExprKind::Yield(value) => ExprKind::List(vec![(**value).clone()]),
        ExprKind::Each { .. } => ExprKind::Generate {
            body: Box::new(e.clone()),
            strict: true,
            sync: false,
            limit: None,
        },
        ExprKind::If(c, yes, no) => {
            ExprKind::If(c.clone(), Box::new(collected(yes)), Box::new(collected(no)))
        }
        ExprKind::Block(stmts, tail) => ExprKind::Block(stmts.clone(), Box::new(collected(tail))),
        ExprKind::Lit(ir::Lit::Unit) => ExprKind::List(Vec::new()),
        _ => return e.clone(),
    };
    ir::Expr::new(kind, e.span)
}

impl Tr {
    pub(super) fn strict_collection(&mut self, e: &ir::Expr) -> Term {
        let ExprKind::Each {
            pat: p,
            source,
            body,
            ..
        } = &e.kind
        else {
            return self.expr(&collected(e));
        };
        let source = self.expr(source);
        let name = self.fresh("collect");
        let list = self.fresh("list");
        let head = self.fresh("head");
        let tail = self.fresh("tail");
        let task = effectful(body);
        let call = |term| {
            if task {
                Term::Await(Rc::new(term))
            } else {
                term
            }
        };
        let next = call(Term::app(Term::Var(name.clone()), Term::Var(tail.clone())));
        let values = self.strict_collection(body);
        let append = Term::apps(Term::prim(Prim::ListConcat), [values, next.clone()]);
        let matched = Term::Match(
            Rc::new(Term::Var(head.clone())),
            Rc::from([
                Arm {
                    pat: pat(p),
                    guard: None,
                    body: append,
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: next,
                },
            ]),
        );
        let step = list_step(&list, head, tail, Term::List(Rc::from([])), matched);
        let step = if task {
            Term::Task(Rc::new(step))
        } else {
            step
        };
        let function = Term::lam(&list, step);
        Term::LetRec(
            name.clone(),
            None,
            Rc::new(function),
            Rc::new(call(Term::app(Term::Var(name), source))),
        )
    }
}
