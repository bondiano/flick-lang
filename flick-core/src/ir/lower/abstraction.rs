//! Keep dictionary-independent lexical control eager when introducing dictionaries.
use crate::ir::{Expr, ExprKind, Stmt};
use crate::types::Dict;

fn given(dict: &Dict, names: &[String]) -> bool {
    match dict {
        Dict::Given(name) => names.contains(name),
        Dict::Super { dict, .. } => given(dict, names),
        Dict::Instance { args, .. } | Dict::Structural { fields: args, .. } => {
            args.iter().any(|d| given(d, names))
        }
        _ => false,
    }
}
fn depends(e: &Expr, names: &[String]) -> bool {
    if matches!(&e.kind, ExprKind::Method { dict, .. } if given(dict, names))
        || matches!(&e.kind, ExprKind::Dicts(_, ds) if ds.iter().any(|d| given(d, names)))
    {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= depends(c, names));
    found
}
fn exits(e: &Expr) -> bool {
    if matches!(e.kind, ExprKind::LoopExit { .. }) {
        return true;
    }
    if matches!(
        e.kind,
        ExprKind::Lambda { .. }
            | ExprKind::Task(_)
            | ExprKind::ControlLoop { .. }
            | ExprKind::Generate { .. }
    ) {
        return false;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= exits(c));
    found
}
fn statement(s: &Stmt) -> &Expr {
    match s {
        Stmt::Let(_, e)
        | Stmt::Var(_, e)
        | Stmt::Assign(_, e)
        | Stmt::Expr(e)
        | Stmt::Fun { value: e, .. } => e,
    }
}

/// Only normal branches need dictionaries. An exit must precede the call that
/// would consume the polymorphic value, rather than run inside that consumer.
pub(super) fn abstract_value(names: Vec<String>, body: Expr) -> Expr {
    let span = body.span;
    if exits(&body) {
        let kind = match &body.kind {
            ExprKind::LoopExit { value, .. }
                if value.as_ref().is_none_or(|e| !depends(e, &names)) =>
            {
                return body;
            }
            ExprKind::If(c, a, b) if !depends(c, &names) => Some(ExprKind::If(
                c.clone(),
                Box::new(abstract_value(names.clone(), (**a).clone())),
                Box::new(abstract_value(names.clone(), (**b).clone())),
            )),
            ExprKind::Block(stmts, tail)
                if stmts.iter().all(|s| !depends(statement(s), &names)) =>
            {
                Some(ExprKind::Block(
                    stmts.clone(),
                    Box::new(abstract_value(names.clone(), (**tail).clone())),
                ))
            }
            ExprKind::Match(s, arms)
                if !depends(s, &names)
                    && arms
                        .iter()
                        .all(|a| a.guard.as_ref().is_none_or(|g| !depends(g, &names))) =>
            {
                Some(ExprKind::Match(
                    s.clone(),
                    arms.iter()
                        .cloned()
                        .map(|mut a| {
                            a.body = abstract_value(names.clone(), a.body);
                            a
                        })
                        .collect(),
                ))
            }
            _ => None,
        };
        if let Some(kind) = kind {
            return Expr::new(kind, span);
        }
    }
    Expr::new(ExprKind::Abstract(names, Box::new(body)), span)
}
