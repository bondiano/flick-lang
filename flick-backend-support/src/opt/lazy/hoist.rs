use std::collections::HashSet;
use std::mem;

use super::purity::{no_outer_var_or_exit, pure_body, uses_any_bound};
use super::{is, value};
use crate::Known;
use crate::ir::{Expr, ExprKind, Lit, Pat, Stage, Stmt};
use flick_ir::Lang;

/// O4: hoists pure, loop-invariant lazy cells out of loops (`ControlLoop`,
/// `Loop`, `Each`, and recursive runtime local functions) so iterations share
/// one cell instead of making one each time. Whether anything hoisted.
pub fn hoist(e: &mut Expr, fresh: &mut usize, lang: &Known<'_>) -> bool {
    if !has_lazy(e, lang) {
        return false;
    }
    hoist_in(e, false, fresh, lang)
}

fn has_lazy(e: &Expr, lang: &Known<'_>) -> bool {
    if matches!(&e.kind, ExprKind::App { func, .. } if is(func, Lang::LazyMake, lang) || is(func, Lang::LazyDone, lang))
    {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= has_lazy(c, lang));
    found
}

fn fresh_name(fresh: &mut usize) -> String {
    *fresh += 1;
    format!("$lazy{fresh}")
}

fn hoist_in(e: &mut Expr, in_runtime: bool, fresh: &mut usize, lang: &Known<'_>) -> bool {
    match &mut e.kind {
        ExprKind::Lambda { params, body } => {
            let body_runtime = params.last().is_some_and(|p| p.stage == Stage::Runtime);
            hoist_in(body, body_runtime, fresh, lang)
        }
        ExprKind::Task(body) => hoist_in(body, true, fresh, lang),
        ExprKind::Generate { body, limit, .. } => {
            let mut changed = false;
            if let Some(limit) = limit {
                changed |= hoist_in(limit, in_runtime, fresh, lang);
            }
            changed | hoist_in(body, false, fresh, lang)
        }
        ExprKind::ControlLoop { .. } | ExprKind::Loop { .. } | ExprKind::Each { .. } => {
            hoist_loop(e, in_runtime, fresh, lang)
        }
        ExprKind::Block(stmts, result) => hoist_block(stmts, result, in_runtime, fresh, lang),
        _ => {
            let mut changed = false;
            e.each_child_mut(&mut |c| changed |= hoist_in(c, in_runtime, fresh, lang));
            changed
        }
    }
}

fn hoist_loop(e: &mut Expr, in_runtime: bool, fresh: &mut usize, lang: &Known<'_>) -> bool {
    let mut changed = false;
    let mut bound = HashSet::new();
    let mut hoisted = Vec::new();
    match &mut e.kind {
        ExprKind::ControlLoop {
            condition, body, ..
        } => {
            if let Some(cond) = condition {
                changed |= hoist_in(cond, in_runtime, fresh, lang);
            }
            changed |= hoist_in(body, in_runtime, fresh, lang);
            if in_runtime {
                if let Some(cond) = condition {
                    collect_expr(cond, &mut bound, &mut hoisted, fresh, lang);
                }
                collect_expr(body, &mut bound, &mut hoisted, fresh, lang);
            }
        }
        ExprKind::Loop { condition, body } => {
            changed |= hoist_in(condition, in_runtime, fresh, lang);
            changed |= hoist_in(body, in_runtime, fresh, lang);
            if in_runtime {
                collect_expr(condition, &mut bound, &mut hoisted, fresh, lang);
                collect_expr(body, &mut bound, &mut hoisted, fresh, lang);
            }
        }
        ExprKind::Each {
            stop,
            pat,
            source,
            body,
            ..
        } => {
            changed |= hoist_in(source, in_runtime, fresh, lang);
            if let Some(stop) = stop {
                changed |= hoist_in(stop, in_runtime, fresh, lang);
            }
            changed |= hoist_in(body, in_runtime, fresh, lang);
            if in_runtime {
                if let Some(stop) = stop {
                    collect_expr(stop, &mut bound, &mut hoisted, fresh, lang);
                }
                add_pat_binders(pat, &mut bound);
                collect_expr(body, &mut bound, &mut hoisted, fresh, lang);
            }
        }
        _ => return false,
    }
    if hoisted.is_empty() {
        return changed;
    }
    wrap_hoisted(e, hoisted);
    true
}

fn hoist_block(
    stmts: &mut Vec<Stmt>,
    result: &mut Expr,
    in_runtime: bool,
    fresh: &mut usize,
    lang: &Known<'_>,
) -> bool {
    let mut changed = false;
    let mut out = Vec::with_capacity(stmts.len());
    for mut stmt in mem::take(stmts) {
        match &mut stmt {
            Stmt::Fun {
                name,
                rec: true,
                dicts,
                value,
                ..
            } if in_runtime && dicts.is_empty() => {
                if let ExprKind::Lambda { params, body } = &mut value.kind
                    && params.last().is_some_and(|p| p.stage == Stage::Runtime)
                {
                    changed |= hoist_in(body, true, fresh, lang);
                    let mut bound = HashSet::new();
                    bound.insert(name.clone());
                    for param in params {
                        add_pat_binders(&param.pat, &mut bound);
                    }
                    let mut hoisted = Vec::new();
                    collect_expr(body, &mut bound, &mut hoisted, fresh, lang);
                    if !hoisted.is_empty() {
                        out.extend(hoisted);
                        changed = true;
                    }
                } else {
                    changed |= hoist_in(value, in_runtime, fresh, lang);
                }
            }
            Stmt::Let { value: v, .. }
            | Stmt::Var { value: v, .. }
            | Stmt::Assign(_, v)
            | Stmt::Expr(v)
            | Stmt::Fun { value: v, .. } => {
                changed |= hoist_in(v, in_runtime, fresh, lang);
            }
        }
        out.push(stmt);
    }
    *stmts = out;
    changed | hoist_in(result, in_runtime, fresh, lang)
}

fn wrap_hoisted(e: &mut Expr, hoisted: Vec<Stmt>) {
    let span = e.span;
    let ty = e.ty.clone();
    let unit = Expr::new(ExprKind::Lit(Lit::Unit), span);
    let inner = mem::replace(e, unit);
    *e = Expr::with_ty(ExprKind::Block(hoisted, Box::new(inner)), span, ty);
}

fn add_pat_binders(pat: &Pat, bound: &mut HashSet<String>) -> Vec<String> {
    let mut names = Vec::new();
    pat.binders(&mut names);
    let mut added = Vec::new();
    for name in names {
        let owned = name.to_owned();
        if bound.insert(owned.clone()) {
            added.push(owned);
        }
    }
    added
}

fn pop_binders(bound: &mut HashSet<String>, added: Vec<String>) {
    for name in added {
        bound.remove(&name);
    }
}

/// Whether `e` is a pure, loop-invariant lazy cell (`lazy_make (task body)`
/// or `lazy_done v`) that may hoist out of the current loop.
fn hoistable(e: &Expr, bound: &HashSet<String>, lang: &Known<'_>) -> bool {
    let ExprKind::App { func, arg, .. } = &e.kind else {
        return false;
    };
    if is(func, Lang::LazyMake, lang)
        && let ExprKind::Task(body) = &arg.kind
    {
        return pure_body(body, &HashSet::new(), &HashSet::new()) && !uses_any_bound(body, bound);
    }
    if is(func, Lang::LazyDone, lang) && value(arg) {
        return no_outer_var_or_exit(arg, &HashSet::new(), &HashSet::new())
            && !uses_any_bound(arg, bound);
    }
    false
}

/// Whether `arg` directly inside `lazy_force arg` folds straight to a value via
/// O1b + O1c and should stay in place instead of hoisting a cell.
fn direct_value_cell(arg: &Expr, lang: &Known<'_>) -> bool {
    let ExprKind::App { func, arg, .. } = &arg.kind else {
        return false;
    };
    if is(func, Lang::LazyDone, lang) && value(arg) {
        return true;
    }
    is(func, Lang::LazyMake, lang) && matches!(&arg.kind, ExprKind::Task(body) if value(body))
}

#[allow(clippy::too_many_lines)]
fn collect_expr(
    e: &mut Expr,
    bound: &mut HashSet<String>,
    hoisted: &mut Vec<Stmt>,
    fresh: &mut usize,
    lang: &Known<'_>,
) {
    // Keep `lazy_force (lazy_make (task v))` and `lazy_force (lazy_done v)` in
    // place so O1b + O1c folds them directly to `v`.
    if let ExprKind::App { func, arg, .. } = &mut e.kind
        && is(func, Lang::LazyForce, lang)
        && direct_value_cell(arg, lang)
    {
        return;
    }
    if hoistable(e, bound, lang) {
        let name = fresh_name(fresh);
        let span = e.span;
        let ty = e.ty.clone();
        let replacement = Expr::with_ty(ExprKind::Local(name.clone()), span, ty.clone());
        let cell = mem::replace(e, replacement);
        let bt = ty.map(|t| vec![(name.clone(), t)]).unwrap_or_default();
        hoisted.push(Stmt::let_typed(Pat::Bind(name), bt, cell));
        return;
    }
    match &mut e.kind {
        // Never cross function, task, handler, generator, owner or inner loop
        // boundaries. Inner loops already hoisted their invariant cells to a
        // wrapper block right before them; only `Each.source` runs once per
        // outer iteration without having been hoisted by `Each` itself.
        ExprKind::Lambda { .. }
        | ExprKind::Task(_)
        | ExprKind::Abstract(..)
        | ExprKind::Generate { .. }
        | ExprKind::Handle(_)
        | ExprKind::Within(..)
        | ExprKind::WithOwnerEvidence(..)
        | ExprKind::ControlLoop { .. }
        | ExprKind::Loop { .. } => {}
        ExprKind::Each { source, .. } => {
            collect_expr(source, bound, hoisted, fresh, lang);
        }
        ExprKind::Block(stmts, result) => {
            let mut added = Vec::new();
            let mut kept = Vec::with_capacity(stmts.len());
            for mut stmt in mem::take(stmts) {
                match &mut stmt {
                    // Let O1a drop dead/ignored lazy cells rather than hoisting them.
                    Stmt::Let {
                        pat: Pat::Wild,
                        value: v,
                        ..
                    } if hoistable(v, bound, lang) => {
                        kept.push(stmt);
                    }
                    Stmt::Expr(v)
                        if hoistable(v, bound, lang)
                            || matches!(&v.kind, ExprKind::Ignore(inner) if hoistable(inner, bound, lang)) =>
                    {
                        kept.push(stmt);
                    }
                    // Moving `let l = lazy_make ...` directly keeps `l` unique
                    // and avoids wrapper aliases; `l` is now bound before the loop.
                    Stmt::Let {
                        pat: Pat::Bind(_),
                        value: v,
                        ..
                    } if hoistable(v, bound, lang) => {
                        hoisted.push(stmt);
                    }
                    Stmt::Let { pat, value: v, .. } => {
                        collect_expr(v, bound, hoisted, fresh, lang);
                        added.extend(add_pat_binders(pat, bound));
                        kept.push(stmt);
                    }
                    Stmt::Var { name, value: v, .. } => {
                        collect_expr(v, bound, hoisted, fresh, lang);
                        if bound.insert(name.clone()) {
                            added.push(name.clone());
                        }
                        kept.push(stmt);
                    }
                    Stmt::Assign(_, v) | Stmt::Expr(v) => {
                        collect_expr(v, bound, hoisted, fresh, lang);
                        kept.push(stmt);
                    }
                    Stmt::Fun { name, .. } => {
                        if bound.insert(name.clone()) {
                            added.push(name.clone());
                        }
                        kept.push(stmt);
                    }
                }
            }
            collect_expr(result, bound, hoisted, fresh, lang);
            pop_binders(bound, added);
            if kept.is_empty() {
                let unit = Expr::new(ExprKind::Lit(Lit::Unit), e.span);
                *e = mem::replace(&mut **result, unit);
            } else {
                *stmts = kept;
            }
        }
        ExprKind::Match(scrutinee, arms) => {
            collect_expr(scrutinee, bound, hoisted, fresh, lang);
            for arm in arms {
                let added = add_pat_binders(&arm.pat, bound);
                if let Some(guard) = &mut arm.guard {
                    collect_expr(guard, bound, hoisted, fresh, lang);
                }
                collect_expr(&mut arm.body, bound, hoisted, fresh, lang);
                pop_binders(bound, added);
            }
        }
        _ => {
            e.each_child_mut(&mut |c| collect_expr(c, bound, hoisted, fresh, lang));
        }
    }
}
