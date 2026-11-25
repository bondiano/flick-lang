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

/// What a statement evaluates.
fn value(s: &Stmt) -> &Expr {
    match s {
        Stmt::Let { value, .. }
        | Stmt::Var { value, .. }
        | Stmt::Assign(_, value)
        | Stmt::Expr(value)
        | Stmt::Fun { value, .. } => value,
    }
}

fn value_mut(s: &mut Stmt) -> &mut Expr {
    match s {
        Stmt::Let { value, .. }
        | Stmt::Var { value, .. }
        | Stmt::Assign(_, value)
        | Stmt::Expr(value)
        | Stmt::Fun { value, .. } => value,
    }
}

/// Statements that run before the read: none assigns `x`, and no branch or
/// scope of them assigns, as the translation of `LetMut` binds every variable
/// again after one. A direct assignment of another variable is a `set`.
fn before(stmts: &[Stmt], x: &str) -> bool {
    stmts
        .iter()
        .all(|s| !matches!(s, Stmt::Assign(v, _) if v == x) && !assigns(value(s)))
}

/// The one read of `x` in `e` is its `match`, which runs at most once each
/// time `e` does and holds the one assignment of `x`: no loop, function,
/// task or handler but a `try` is around it, and nothing that runs before it
/// in `e` assigns in a branch or a scope.
fn once(e: &Expr, x: &str) -> bool {
    if target(e, x) {
        let (mut reads, mut sets) = (0, 0);
        count(e, x, &mut reads, &mut sets);
        return sets == 1;
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
    match stmts.iter().position(|s| reads(value(s), x) > 0) {
        Some(i) => {
            before(&stmts[..i], x)
                && match &stmts[i] {
                    Stmt::Assign(..) | Stmt::Fun { .. } => false,
                    s => once(value(s), x),
                }
        }
        None => before(stmts, x) && once(result, x),
    }
}

/// The read of `x` becomes `state`, and its assignment the evaluation of
/// what it assigned.
fn rewrite(e: &mut Expr, x: &str, state: &Expr) {
    match &mut e.kind {
        ExprKind::Match(s, _) if matches!(&s.kind, ExprKind::Var(v) if v == x) => {
            **s = state.clone();
        }
        ExprKind::Block(stmts, _) => {
            for stmt in stmts.iter_mut() {
                if let Stmt::Assign(v, value) = stmt
                    && v == x
                {
                    *stmt = Stmt::Expr(mem::replace(value, state.clone()));
                }
            }
        }
        _ => {}
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
        let Stmt::Var {
            name: x,
            value: state,
            ..
        } = &stmts[i]
        else {
            i += 1;
            continue;
        };
        if !matches!(state.kind, ExprKind::Con(_)) {
            i += 1;
            continue;
        }
        let rest = &stmts[i + 1..];
        let mut reads = 0;
        // assignments directly in the block are counted here, the others by `count`
        let mut sets = rest
            .iter()
            .filter(|s| matches!(s, Stmt::Assign(v, _) if v == x))
            .count();
        for e in rest.iter().map(value).chain([&*result]) {
            count(e, x, &mut reads, &mut sets);
        }
        if !(reads == 1 && sets == 1 && once_block(rest, result, x)) {
            i += 1;
            continue;
        }
        let Stmt::Var {
            name: x,
            value: state,
            ..
        } = stmts.remove(i)
        else {
            unreachable!("a variable was found at {i}")
        };
        for stmt in &mut stmts[i..] {
            rewrite(value_mut(stmt), &x, &state);
        }
        rewrite(result, &x, &state);
        changed = true;
    }
    changed
}
