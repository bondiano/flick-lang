//! Lazy cells a program need not make, each a contextual equivalence of the
//! core (`docs/formal/lazy.md#6-optimizations-as-equivalences`):
//!
//! - O1a: `let l = lazy_make t` that nothing reads is not made;
//! - O1b: `lazy_make (task v)` of a value `v` is `lazy_done v`, a cell made
//!   done, whose force runs nothing;
//! - O1c: `lazy_force (lazy_make t)` is `await t`: the cell has no other
//!   owner, so its one force runs the task where it is made; and
//!   `lazy_force (lazy_done v)`, both at once, is `v`;
//! - O2a: after `let y = lazy_force l`, a force of the same `l` is `y`: the
//!   first force returned only once the cell was done, and a done cell
//!   stays done (L2);
//! - O4: a pure, loop-invariant `lazy_make (task e)` (or `lazy_done v`)
//!   inside a loop hoists before the loop, so iterations share one cell.

use std::collections::HashSet;
use std::mem;

use super::walk::{droppable, each_use};
use crate::Known;
use crate::ir::{Expr, ExprKind, Lit, Pat, Stmt};
use flick_ir::{IrType, Lang, TypeRef};

mod hoist;
mod purity;

pub use hoist::hoist;

/// Whether `e` names the lang item `item`.
pub(super) fn is(e: &Expr, item: Lang, lang: &Known<'_>) -> bool {
    matches!(&e.kind, ExprKind::Global { module, name, .. } if lang.is(item, module, name))
}

/// The task of `lazy_make task`.
fn made<'e>(e: &'e Expr, lang: &Known<'_>) -> Option<&'e Expr> {
    match &e.kind {
        ExprKind::App { func, arg, .. } if is(func, Lang::LazyMake, lang) => Some(arg),
        _ => None,
    }
}

/// A value of the core: evaluating it takes no step. A local is one, as the
/// core substitutes values for variables; a mutable variable is not.
pub(super) fn value(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Con(_) | ExprKind::Lambda { .. } => true,
        ExprKind::Tuple(items) | ExprKind::List(items) => items.iter().all(value),
        ExprKind::Record(fields) => fields.iter().all(|(_, v)| value(v)),
        _ => false,
    }
}

/// O1b and O1c at `e`. Whether `e` changed.
pub fn cells(e: &mut Expr, lang: &Known<'_>) -> bool {
    let ExprKind::App { func, arg, .. } = &mut e.kind else {
        return false;
    };
    if is(func, Lang::LazyForce, lang)
        && let ExprKind::App {
            func: make,
            arg: task,
            ..
        } = &mut arg.kind
    {
        let unit = Expr::new(ExprKind::Lit(Lit::Unit), e.span);
        if is(make, Lang::LazyMake, lang) {
            let task = mem::replace(&mut **task, unit);
            e.kind = ExprKind::Await(Box::new(task));
            return true;
        }
        // a cell O1b made done, forced where it is made: its value
        if is(make, Lang::LazyDone, lang) {
            *e = mem::replace(&mut **task, unit);
            return true;
        }
    }
    if is(func, Lang::LazyMake, lang)
        && let ExprKind::Task(body) = &mut arg.kind
        && value(body)
        && let Some((module, name)) = lang.get(Lang::LazyDone)
    {
        let body = mem::replace(&mut **body, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
        let type_args = match &mut func.kind {
            ExprKind::Global { type_args, .. } => mem::take(type_args),
            _ => Vec::new(),
        };
        func.ty = match (func.ty.as_deref(), body.ty.as_ref()) {
            (Some(IrType::Fun { stage, result, .. }), Some(param)) => {
                Some(TypeRef::new(IrType::Fun {
                    param: param.clone(),
                    stage: *stage,
                    result: result.clone(),
                }))
            }
            _ => None,
        };
        func.kind = ExprKind::Global {
            module: module.to_owned(),
            name: name.to_owned(),
            type_args,
        };
        **arg = body;
        return true;
    }
    false
}

/// O1a in a block: a cell made of a task or a name, which nothing reads or
/// which is only dropped. Whether a statement went.
pub fn unnamed(stmts: &mut Vec<Stmt>, result: &Expr, lang: &Known<'_>) -> bool {
    let unread = |v: &Expr| made(v, lang).is_some_and(droppable);
    let dropped = |v: &Expr| match &v.kind {
        ExprKind::Ignore(inner) => unread(inner),
        _ => unread(v),
    };
    let any = |s: &Stmt| {
        matches!(s, Stmt::Let { value: v, .. } if unread(v))
            || matches!(s, Stmt::Expr(v) if dropped(v))
    };
    if !stmts.iter().any(any) {
        return false;
    }
    let mut read: HashSet<&str> = HashSet::new();
    each_use(result, &mut |l| {
        read.insert(l);
    });
    let mut keep = vec![true; stmts.len()];
    for (i, stmt) in stmts.iter().enumerate().rev() {
        let drop = match stmt {
            Stmt::Let {
                pat: p, value: v, ..
            } if unread(v) && p.irrefutable() => {
                let mut bound = Vec::new();
                p.binders(&mut bound);
                bound.iter().all(|n| !read.contains(n))
            }
            Stmt::Expr(v) => dropped(v),
            _ => false,
        };
        if drop {
            keep[i] = false;
            continue;
        }
        match stmt {
            Stmt::Let { value: v, .. }
            | Stmt::Var { value: v, .. }
            | Stmt::Assign(_, v)
            | Stmt::Expr(v)
            | Stmt::Fun { value: v, .. } => each_use(v, &mut |l| {
                read.insert(l);
            }),
        }
    }
    let before = stmts.len();
    let mut keep = keep.into_iter();
    stmts.retain(|_| keep.next().expect("one for each statement"));
    stmts.len() != before
}

/// The local whose cell `e` forces: `lazy_force l`.
fn forced<'e>(e: &'e Expr, lang: &Known<'_>) -> Option<&'e str> {
    match &e.kind {
        ExprKind::App { func, arg, .. } if is(func, Lang::LazyForce, lang) => match &arg.kind {
            ExprKind::Local(l) => Some(l),
            _ => None,
        },
        _ => None,
    }
}

/// Each `lazy_force l` in `e` replaced by `y`. Whether one was.
fn reuse(e: &mut Expr, l: &str, y: &str, lang: &Known<'_>) -> bool {
    if forced(e, lang) == Some(l) {
        e.kind = ExprKind::Local(y.to_owned());
        return true;
    }
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= reuse(c, l, y, lang));
    changed
}

/// O2a in a block: `let y = lazy_force l`, then `lazy_force l` in what comes
/// after it, there or in anything nested, is `y`. Locals are unique and
/// immutable, so `l` names the same cell, which the first force left done
/// with `y` before anything after it ran. Whether a force went.
pub fn forced_again(stmts: &mut [Stmt], result: &mut Expr, lang: &Known<'_>) -> bool {
    let mut changed = false;
    for i in 0..stmts.len() {
        let (head, rest) = stmts.split_at_mut(i + 1);
        let Stmt::Let {
            pat: Pat::Bind(y),
            value: v,
            ..
        } = &head[i]
        else {
            continue;
        };
        let Some(l) = forced(v, lang) else {
            continue;
        };
        for stmt in rest.iter_mut() {
            match stmt {
                Stmt::Let { value: v, .. }
                | Stmt::Var { value: v, .. }
                | Stmt::Assign(_, v)
                | Stmt::Expr(v)
                | Stmt::Fun { value: v, .. } => changed |= reuse(v, l, y, lang),
            }
        }
        changed |= reuse(result, l, y, lang);
    }
    changed
}
