//! Lazy cells a program need not make, each a contextual equivalence of the
//! core (`docs/formal/lazy.md#6-optimizations-as-equivalences`):
//!
//! - O1a: `let l = lazy_make t` that nothing reads is not made;
//! - O1b: `lazy_make (task v)` of a value `v` is `lazy_done v`, a cell made
//!   done, whose force runs nothing;
//! - O1c: `lazy_force (lazy_make t)` is `await t`: the cell has no other
//!   owner, so its one force runs the task where it is made; and
//!   `lazy_force (lazy_done v)`, both at once, is `v`.

use std::collections::HashSet;
use std::mem;

use super::walk::{droppable, each_use};
use crate::Known;
use crate::ir::{Expr, ExprKind, Lit, Stmt};
use flick_ir::Lang;

/// Whether `e` names the lang item `item`.
fn is(e: &Expr, item: Lang, lang: &Known<'_>) -> bool {
    matches!(&e.kind, ExprKind::Global { module, name } if lang.is(item, module, name))
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
fn value(e: &Expr) -> bool {
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
        func.kind = ExprKind::Global {
            module: module.to_owned(),
            name: name.to_owned(),
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
        matches!(s, Stmt::Let(_, v) if unread(v)) || matches!(s, Stmt::Expr(v) if dropped(v))
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
            Stmt::Let(p, v) if unread(v) && p.irrefutable() => {
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
            Stmt::Let(_, v)
            | Stmt::Var(_, v)
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
