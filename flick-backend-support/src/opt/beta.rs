//! Beta reduction and block hoisting out of call positions.

use std::mem;

use super::inline::{bind, exits, simple};
use crate::ir::{Expr, ExprKind, Lit, Stage};

/// Reduces `(fun x -> body) arg` to `{ let x = arg; body }`, and hoists a
/// block out of the callee position of an application; whether `e` changed.
pub fn beta(e: &mut Expr, preserve_types: bool) -> bool {
    // `{ s; f } a` is `{ s; f a }` when `f` is evaluated first: `f` is then
    // visible to beta reduction and inlining
    if let ExprKind::App {
        func,
        arg_first: false,
        ..
    } = &mut e.kind
        && let ExprKind::Block(stmts, value) = &mut func.kind
    {
        let stmts = mem::take(stmts);
        let value = mem::replace(&mut **value, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
        let span = e.span;
        let ty = preserve_types.then(|| e.ty.clone()).flatten();
        let ExprKind::App { arg, stage, .. } = mem::replace(&mut e.kind, ExprKind::Lit(Lit::Unit))
        else {
            unreachable!("matched above")
        };
        let app = ExprKind::App {
            func: Box::new(value),
            arg,
            stage,
            arg_first: false,
        };
        *e = Expr::with_ty(
            ExprKind::Block(stmts, Box::new(Expr::with_ty(app, span, ty.clone()))),
            span,
            ty,
        );
        return true;
    }
    let ExprKind::App {
        func, arg, stage, ..
    } = &mut e.kind
    else {
        return false;
    };
    // `a |> (fun x -> …)` evaluates `a` first too: the function value is inert
    let ExprKind::Lambda { params, body } = &mut func.kind else {
        return false;
    };
    if params
        .first()
        .is_none_or(|p| p.stage != *stage || !simple(&p.pat))
        // a call of an effectful stage runs the body, which then runs here,
        // in the stage of the call; a stage before others runs nothing
        || (*stage != Stage::Pure && params.len() > 1)
        // an `o?` returns from the function it is in, not from the one around
        || (params.len() == 1 && exits(body))
    {
        return false;
    }
    let ty = preserve_types.then(|| e.ty.clone()).flatten();
    let first = params.remove(0);
    let arg = mem::replace(&mut **arg, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
    let rest = if params.is_empty() {
        mem::replace(&mut **body, Expr::new(ExprKind::Lit(Lit::Unit), e.span))
    } else {
        let mut f = mem::replace(&mut **func, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
        if preserve_types {
            f.ty.clone_from(&ty);
        }
        f
    };
    let ty = preserve_types
        .then(|| ty.or_else(|| rest.ty.clone()))
        .flatten();
    *e = Expr::with_ty(
        ExprKind::Block(vec![bind(&first, arg)], Box::new(rest)),
        e.span,
        ty,
    );
    true
}
