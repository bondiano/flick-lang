//! O2b of `docs/formal/lazy.md#6-optimizations-as-equivalences`: the state
//! of a `let lazy` read once. `let lazy x = e` lowers to a task `x$run` and a
//! variable `x`, `None` until the `match` of an `await` runs the task and sets
//! it. A read of `x` before any assignment of it gives `None`, so its `match`
//! takes the arm that runs the task (`lzOnce` of the `LetMut` calculus).
//! When that is the only read, `x` is read no more and goes: ordinary
//! dead-variable elimination.

use std::mem;

use crate::ir::{Expr, ExprKind, Stmt};

/// How many times `e` reads the variable `x`, and how many times it assigns it.
fn count(e: &Expr, x: &str, reads: &mut usize, sets: &mut usize) {
    match &e.kind {
        ExprKind::Var(v) if v == x => *reads += 1,
        ExprKind::Block(stmts, _) => {
            *sets += stmts
                .iter()
                .filter(|s| matches!(s, Stmt::Assign(v, _) if v == x))
                .count();
        }
        _ => {}
    }
    e.each_child(&mut |c| count(c, x, reads, sets));
}

fn reads(e: &Expr, x: &str) -> usize {
    let (mut reads, mut sets) = (0, 0);
    count(e, x, &mut reads, &mut sets);
    reads
}

/// Whether `e` assigns a variable anywhere.
fn assigns(e: &Expr) -> bool {
    if let ExprKind::Block(stmts, _) = &e.kind
        && stmts.iter().any(|s| matches!(s, Stmt::Assign(..)))
    {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= assigns(c));
    found
}

/// The `match` on `x` itself.
fn target(e: &Expr, x: &str) -> bool {
    matches!(&e.kind, ExprKind::Match(s, _) if matches!(&s.kind, ExprKind::Var(v) if v == x))
}

/// Statements that run before the read: no branch or scope of them assigns,
/// as the translation of `LetMut` binds every variable again after one. A
/// direct assignment of another variable is a `set`.
fn before(stmts: &[Stmt], x: &str) -> bool {
    stmts.iter().all(|s| match s {
        Stmt::Assign(v, value) => v != x && !assigns(value),
        Stmt::Let(_, v) | Stmt::Var(_, v) | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => {
            !assigns(v)
        }
    })
}

/// The one read of `x` in `e` is its `match`, which runs at most once each
/// time `e` does: no loop, function, task or handler is around it, and nothing
/// that runs before it in `e` assigns in a branch or a scope.
fn once(e: &Expr, x: &str) -> bool {
    if target(e, x) {
        return true;
    }
    match &e.kind {
        ExprKind::Block(stmts, result) => once_block(stmts, result, x),
        ExprKind::Handle(h) if h.is_try() => reads(&h.body, x) == 1 && once(&h.body, x),
        ExprKind::If(..)
        | ExprKind::Match(..)
        | ExprKind::App { .. }
        | ExprKind::Binary(..)
        | ExprKind::Not(_)
        | ExprKind::Unwrap(_)
        | ExprKind::Neg(..)
        | ExprKind::Ignore(_)
        | ExprKind::Await(_)
        | ExprKind::Tuple(_)
        | ExprKind::List(_)
        | ExprKind::Record(_)
        | ExprKind::Update(..)
        | ExprKind::Field(..) => {
            let mut ok = true;
            e.each_child(&mut |c| {
                ok &= if reads(c, x) > 0 {
                    once(c, x)
                } else {
                    !assigns(c)
                };
            });
            ok
        }
        _ => false,
    }
}

/// `once` of a block: the statement that reads `x`, or its value, after
/// statements that do not assign in a branch or a scope.
fn once_block(stmts: &[Stmt], result: &Expr, x: &str) -> bool {
    let reading = stmts.iter().position(|s| match s {
        Stmt::Let(_, v)
        | Stmt::Var(_, v)
        | Stmt::Assign(_, v)
        | Stmt::Expr(v)
        | Stmt::Fun { value: v, .. } => reads(v, x) > 0,
    });
    match reading {
        Some(i) => {
            before(&stmts[..i], x)
                && match &stmts[i] {
                    Stmt::Let(_, v) | Stmt::Var(_, v) | Stmt::Expr(v) => once(v, x),
                    Stmt::Assign(..) | Stmt::Fun { .. } => false,
                }
        }
        None => before(stmts, x) && once(result, x),
    }
}

/// Each statement's value and the block's, in order.
fn each_part<'e>(stmts: &'e [Stmt], result: &'e Expr, f: &mut dyn FnMut(&'e Expr)) {
    for stmt in stmts {
        match stmt {
            Stmt::Let(_, v)
            | Stmt::Var(_, v)
            | Stmt::Assign(_, v)
            | Stmt::Expr(v)
            | Stmt::Fun { value: v, .. } => f(v),
        }
    }
    f(result);
}

/// The read of `x` becomes `state`, and its assignment the evaluation of
/// what it assigned.
fn rewrite(e: &mut Expr, x: &str, state: &Expr) {
    if let ExprKind::Match(s, _) = &mut e.kind
        && matches!(&s.kind, ExprKind::Var(v) if v == x)
    {
        **s = state.clone();
    }
    if let ExprKind::Block(stmts, _) = &mut e.kind {
        for stmt in stmts.iter_mut() {
            if let Stmt::Assign(v, value) = stmt
                && v == x
            {
                *stmt = Stmt::Expr(mem::replace(value, state.clone()));
            }
        }
    }
    e.each_child_mut(&mut |c| rewrite(c, x, state));
}

/// O2b in a block: `let mut x = None`, read once, by a `match` that runs at
/// most once, before which nothing assigns `x` or assigns in a branch or a
/// scope, and assigned once, in that `match`. Whether one went.
pub fn state_once(stmts: &mut Vec<Stmt>, result: &mut Expr) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let Stmt::Var(x, state) = &stmts[i] else {
            i += 1;
            continue;
        };
        if !matches!(state.kind, ExprKind::Con(_)) {
            i += 1;
            continue;
        }
        let (rest, value) = (&stmts[i + 1..], &*result);
        let (mut reads, mut sets, mut inside) = (0, 0, 0);
        // assignments directly in the block are counted here, the others by `count`
        sets += rest
            .iter()
            .filter(|s| matches!(s, Stmt::Assign(v, _) if v == x))
            .count();
        each_part(rest, value, &mut |v| {
            count(v, x, &mut reads, &mut sets);
            find(v, x, &mut |m| {
                let mut r = 0;
                count(m, x, &mut r, &mut inside);
            });
        });
        if !(reads == 1 && sets == 1 && inside == 1 && once_block(rest, value, x)) {
            i += 1;
            continue;
        }
        let Stmt::Var(x, state) = stmts.remove(i) else {
            unreachable!("a variable was found at {i}")
        };
        for stmt in &mut stmts[i..] {
            match stmt {
                Stmt::Let(_, v)
                | Stmt::Var(_, v)
                | Stmt::Assign(_, v)
                | Stmt::Expr(v)
                | Stmt::Fun { value: v, .. } => rewrite(v, &x, &state),
            }
        }
        rewrite(result, &x, &state);
        changed = true;
    }
    changed
}

/// Calls `f` on the `match` on `x` in `e`.
fn find<'e>(e: &'e Expr, x: &str, f: &mut dyn FnMut(&'e Expr)) {
    if target(e, x) {
        f(e);
    }
    e.each_child(&mut |c| find(c, x, f));
}
