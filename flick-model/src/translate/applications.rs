//! Keep polymorphic operands in application position while propagating exits.
use super::{
    Tr,
    expr::{is_global, layer_run},
};
use crate::syntax::Term;
use flick_core::ir::{self, ExprKind};
use std::rc::Rc;

/// Put the call only in normal leaves; an exiting leaf never calls it.
fn argument(call: &ir::Expr, value: &ir::Expr) -> ir::Expr {
    let mut out = value.clone();
    out.kind = match &value.kind {
        ExprKind::LoopExit { .. } => return out,
        ExprKind::If(c, a, b) => ExprKind::If(
            c.clone(),
            Box::new(argument(call, a)),
            Box::new(argument(call, b)),
        ),
        ExprKind::Block(stmts, tail) => {
            ExprKind::Block(stmts.clone(), Box::new(argument(call, tail)))
        }
        ExprKind::Match(s, arms) => ExprKind::Match(
            s.clone(),
            arms.iter()
                .cloned()
                .map(|mut a| {
                    a.body = argument(call, &a.body);
                    a
                })
                .collect(),
        ),
        _ => {
            let mut leaf = call.clone();
            let ExprKind::App { arg, .. } = &mut leaf.kind else {
                unreachable!()
            };
            **arg = value.clone();
            return leaf;
        }
    };
    out
}

impl Tr {
    pub(super) fn loop_completion(&self, e: &ir::Expr, id: &str) -> bool {
        if matches!(&e.kind, ExprKind::LoopExit { id: target, .. } if target == id)
            || matches!(&e.kind, ExprKind::App { func, .. } if matches!(&func.kind, ExprKind::Local(n) if self.resumptions.iter().any(|(r, target)| r == n && target == id)))
        {
            return true;
        }
        if matches!(
            e.kind,
            ExprKind::ControlLoop { .. } | ExprKind::Generate { .. }
        ) {
            return false;
        }
        let mut found = false;
        e.each_child(&mut |c| found |= self.loop_completion(c, id));
        found
    }
    pub(super) fn loop_application(
        &mut self,
        e: &ir::Expr,
        id: &str,
        xs: &[String],
    ) -> Option<Term> {
        let ExprKind::App {
            func,
            arg,
            arg_first,
            ..
        } = &e.kind
        else {
            return None;
        };
        if is_global(func, "Scope", "run")
            || is_global(func, "Scope", "current")
            || layer_run(func).is_some()
        {
            return None;
        }
        if !matches!(
            arg.kind,
            ExprKind::If(..)
                | ExprKind::Match(..)
                | ExprKind::Block(..)
                | ExprKind::LoopExit { .. }
        ) || !self.loop_completion(arg, id)
        {
            return None;
        }
        if *arg_first {
            return Some(self.loop_value(&argument(e, arg), id, xs));
        }
        Some(self.loop_with(func, id, xs, &|tr, value| {
            let name = tr.fresh("function");
            let mut call = e.clone();
            let ExprKind::App { func, .. } = &mut call.kind else {
                unreachable!()
            };
            **func = ir::Expr::new(ExprKind::Local(name.to_string()), e.span);
            let body = tr.loop_value(&argument(&call, arg), id, xs);
            Term::Let(name, None, Rc::new(value), Rc::new(body))
        }))
    }
}
