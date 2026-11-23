use std::collections::HashSet;

use crate::ir::{Expr, ExprKind, Stage, Stmt};
use flick_ir::Dict;

/// Whether evaluating `e` is strictly pure (stage `P`): only pure applications,
/// no effects/services/errors, no `Ref`/runtime calls, no owner reads, and no
/// read/write of a mutable variable or exit of a loop declared outside `e`.
pub(super) fn pure_body(
    e: &Expr,
    inner_vars: &HashSet<String>,
    inner_loops: &HashSet<String>,
) -> bool {
    match &e.kind {
        ExprKind::Lit(_)
        | ExprKind::Local(_)
        | ExprKind::Global { .. }
        | ExprKind::Method { .. }
        | ExprKind::Con(_)
        | ExprKind::ServiceTag(_)
        | ExprKind::Label(_) => true,
        ExprKind::Var(name) => inner_vars.contains(name),
        ExprKind::Lambda { body, .. } | ExprKind::Task(body) | ExprKind::Abstract(_, body) => {
            no_outer_var_or_exit(body, inner_vars, inner_loops)
        }
        ExprKind::App {
            func, arg, stage, ..
        } => {
            *stage == Stage::Pure
                && pure_body(func, inner_vars, inner_loops)
                && pure_body(arg, inner_vars, inner_loops)
        }
        ExprKind::Block(stmts, result) => pure_block(stmts, result, inner_vars, inner_loops),
        ExprKind::ControlLoop {
            id,
            condition,
            body,
            ..
        } => {
            let mut loops = inner_loops.clone();
            loops.insert(id.clone());
            condition
                .as_ref()
                .is_none_or(|c| pure_body(c, inner_vars, &loops))
                && pure_body(body, inner_vars, &loops)
        }
        ExprKind::LoopExit { id, value } => {
            inner_loops.contains(id)
                && value
                    .as_ref()
                    .is_none_or(|v| pure_body(v, inner_vars, inner_loops))
        }
        ExprKind::Generate {
            body,
            strict,
            sync,
            limit,
        } => {
            !*sync
                && limit
                    .as_ref()
                    .is_none_or(|l| pure_body(l, inner_vars, inner_loops))
                && if *strict {
                    pure_body(body, inner_vars, inner_loops)
                } else {
                    no_outer_var_or_exit(body, inner_vars, inner_loops)
                }
        }
        ExprKind::Perform { .. }
        | ExprKind::Await(_)
        | ExprKind::Within(..)
        | ExprKind::ReadOwnerEvidence
        | ExprKind::WithOwnerEvidence(..)
        | ExprKind::Handle(_)
        | ExprKind::Unwrap(_) => false,
        ExprKind::Dicts(..)
        | ExprKind::Field(..)
        | ExprKind::Not(_)
        | ExprKind::Neg(..)
        | ExprKind::Ignore(_)
        | ExprKind::Yield(_)
        | ExprKind::Lift(..)
        | ExprKind::Binary(..)
        | ExprKind::If(..)
        | ExprKind::Match(..)
        | ExprKind::Tuple(_)
        | ExprKind::List(_)
        | ExprKind::Record(_)
        | ExprKind::Dictionary(_)
        | ExprKind::Update(..)
        | ExprKind::Loop { .. }
        | ExprKind::Each { .. } => {
            let mut ok = true;
            e.each_child(&mut |c| ok &= pure_body(c, inner_vars, inner_loops));
            ok
        }
    }
}

fn pure_block(
    stmts: &[Stmt],
    result: &Expr,
    inner_vars: &HashSet<String>,
    inner_loops: &HashSet<String>,
) -> bool {
    let mut vars = inner_vars.clone();
    for stmt in stmts {
        match stmt {
            Stmt::Let { value: v, .. } | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => {
                if !pure_body(v, &vars, inner_loops) {
                    return false;
                }
            }
            Stmt::Var { name, value: v, .. } => {
                if !pure_body(v, &vars, inner_loops) {
                    return false;
                }
                vars.insert(name.clone());
            }
            Stmt::Assign(name, v) => {
                if !vars.contains(name) || !pure_body(v, &vars, inner_loops) {
                    return false;
                }
            }
        }
    }
    pure_body(result, &vars, inner_loops)
}

/// Whether `e` (a deferred body inside a pure value such as a lambda or task)
/// never reads/writes a mutable variable or exits a loop from outside the lazy.
pub(super) fn no_outer_var_or_exit(
    e: &Expr,
    inner_vars: &HashSet<String>,
    inner_loops: &HashSet<String>,
) -> bool {
    match &e.kind {
        ExprKind::Var(name) => inner_vars.contains(name),
        ExprKind::LoopExit { id, value } => {
            inner_loops.contains(id)
                && value
                    .as_ref()
                    .is_none_or(|v| no_outer_var_or_exit(v, inner_vars, inner_loops))
        }
        ExprKind::ControlLoop {
            id,
            condition,
            body,
            ..
        } => {
            let mut loops = inner_loops.clone();
            loops.insert(id.clone());
            condition
                .as_ref()
                .is_none_or(|c| no_outer_var_or_exit(c, inner_vars, &loops))
                && no_outer_var_or_exit(body, inner_vars, &loops)
        }
        ExprKind::Block(stmts, result) => {
            let mut vars = inner_vars.clone();
            for stmt in stmts {
                match stmt {
                    Stmt::Let { value: v, .. } | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => {
                        if !no_outer_var_or_exit(v, &vars, inner_loops) {
                            return false;
                        }
                    }
                    Stmt::Var { name, value: v, .. } => {
                        if !no_outer_var_or_exit(v, &vars, inner_loops) {
                            return false;
                        }
                        vars.insert(name.clone());
                    }
                    Stmt::Assign(name, v) => {
                        if !vars.contains(name) || !no_outer_var_or_exit(v, &vars, inner_loops) {
                            return false;
                        }
                    }
                }
            }
            no_outer_var_or_exit(result, &vars, inner_loops)
        }
        _ => {
            let mut ok = true;
            e.each_child(&mut |c| ok &= no_outer_var_or_exit(c, inner_vars, inner_loops));
            ok
        }
    }
}

fn dict_uses(dict: &Dict, bound: &HashSet<String>) -> bool {
    match dict {
        Dict::Given(name) | Dict::Field(name) | Dict::Opened { binder: name, .. } => {
            bound.contains(name)
        }
        Dict::Instance { args, .. } | Dict::Structural { fields: args, .. } => {
            args.iter().any(|d| dict_uses(d, bound))
        }
        Dict::Super { dict, .. } => dict_uses(dict, bound),
        Dict::Value { .. } | Dict::Primitive { .. } | Dict::Site { .. } | Dict::Error => false,
    }
}

/// Whether `e` references any name in `bound` (locals, mutable variables,
/// owner names, dictionary metadata, or loop exit targets).
pub(super) fn uses_any_bound(e: &Expr, bound: &HashSet<String>) -> bool {
    if bound.is_empty() {
        return false;
    }
    match &e.kind {
        ExprKind::Local(name)
        | ExprKind::Var(name)
        | ExprKind::Within(name, _)
        | ExprKind::LoopExit { id: name, .. }
            if bound.contains(name) =>
        {
            return true;
        }
        ExprKind::Method { dict, .. } if dict_uses(dict, bound) => return true,
        ExprKind::Dicts(_, dicts) if dicts.iter().any(|d| dict_uses(d, bound)) => return true,
        ExprKind::Block(stmts, _)
            if stmts
                .iter()
                .any(|s| matches!(s, Stmt::Assign(name, _) if bound.contains(name))) =>
        {
            return true;
        }
        _ => {}
    }
    let mut found = false;
    e.each_child(&mut |c| found |= uses_any_bound(c, bound));
    found
}
