//! What the passes ask of an expression: how a local is used, whether the
//! expression may be dropped or moved, its size; and the rewrites they share:
//! substitution and renaming of locals.

use std::collections::HashMap;

use crate::ir::{BinOp, Expr, ExprKind, Pat, Stage, Stmt};
use crate::numeric::{Arith, Num};
use crate::types::Constructor;

/// How often `e` reads the local `name`.
pub fn uses(e: &Expr, name: &str) -> usize {
    let mut n = 0;
    count(e, name, &mut n);
    n
}

fn count(e: &Expr, name: &str, n: &mut usize) {
    match &e.kind {
        ExprKind::Local(l) | ExprKind::Within(l, _) if l == name => *n += 1,
        _ => {}
    }
    e.each_child(&mut |c| count(c, name, n));
}

/// How often `e` reads `name` where it is evaluated exactly once whenever
/// `e` is: not inside a function, task, branch, arm or handler.
pub fn strict_uses(e: &Expr, name: &str) -> usize {
    match &e.kind {
        ExprKind::Local(l) => usize::from(l == name),
        ExprKind::Lambda { .. }
        | ExprKind::Task(_)
        | ExprKind::Scope { .. }
        | ExprKind::Within(..)
        | ExprKind::Provide(..)
        | ExprKind::Lift(..)
        | ExprKind::Abstract(..)
        | ExprKind::Generate { strict: false, .. } => 0,
        // the body runs once per element, maybe never
        ExprKind::Each { source, .. } => strict_uses(source, name),
        ExprKind::If(c, _, _) => strict_uses(c, name),
        ExprKind::Match(s, _) | ExprKind::Try(s, _) => strict_uses(s, name),
        // `&&` and `||` may skip their right operand
        ExprKind::Binary(BinOp::And | BinOp::Or, l, _) => strict_uses(l, name),
        ExprKind::Block(stmts, result) => {
            let in_stmts: usize = stmts
                .iter()
                .map(|s| match s {
                    Stmt::Let(_, v) | Stmt::Expr(v) => strict_uses(v, name),
                    Stmt::Fun { .. } => 0,
                })
                .sum();
            in_stmts + strict_uses(result, name)
        }
        _ => {
            let mut n = 0;
            e.each_child(&mut |c| n += strict_uses(c, name));
            n
        }
    }
}

/// Replaces each read of the local `name` in `e` with `value`.
pub fn substitute(e: &mut Expr, name: &str, value: &Expr) {
    if matches!(&e.kind, ExprKind::Local(l) if l == name) {
        *e = value.clone();
        return;
    }
    e.each_child_mut(&mut |c| substitute(c, name, value));
}

/// The number of nodes of `e`, patterns included: a `match` emits a test
/// per pattern node.
pub fn size(e: &Expr) -> usize {
    let mut n = 1;
    match &e.kind {
        ExprKind::Match(_, arms) | ExprKind::Try(_, arms) => {
            n += arms.iter().map(|a| pat_size(&a.pat)).sum::<usize>();
        }
        ExprKind::Each { pat, .. } => n += pat_size(pat),
        ExprKind::Block(stmts, _) => {
            n += stmts
                .iter()
                .map(|s| match s {
                    Stmt::Let(p, _) => pat_size(p),
                    _ => 0,
                })
                .sum::<usize>();
        }
        _ => {}
    }
    e.each_child(&mut |c| n += size(c));
    n
}

fn pat_size(p: &Pat) -> usize {
    match p {
        Pat::Wild | Pat::Bind(_) => 0,
        Pat::Lit(_) => 1,
        Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
            1 + items.iter().map(pat_size).sum::<usize>()
        }
        Pat::List(items, rest) => {
            1 + items
                .iter()
                .chain(rest.as_deref())
                .map(pat_size)
                .sum::<usize>()
        }
        Pat::Record(fields) => 1 + fields.iter().map(|(_, p)| pat_size(p)).sum::<usize>(),
    }
}

/// Whether evaluating `e` can do nothing observable: no effect, no throw,
/// no divergence, no allocation anyone could tell apart. Such an expression
/// may be dropped when its value is unused.
pub fn droppable(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(_)
        | ExprKind::Local(_)
        | ExprKind::Global { .. }
        | ExprKind::Con(_)
        | ExprKind::Label(_)
        | ExprKind::ServiceTag(_)
        | ExprKind::Lambda { .. }
        // a task or a sequence is a description: building it runs nothing
        | ExprKind::Task(_)
        | ExprKind::Generate {
            strict: false,
            limit: None,
            ..
        } => true,
        ExprKind::Tuple(items) | ExprKind::List(items) => items.iter().all(droppable),
        ExprKind::Record(fields) => fields.iter().all(|(_, v)| droppable(v)),
        ExprKind::Field(inner, _) | ExprKind::Not(inner) | ExprKind::Neg(_, inner) => droppable(inner),
        ExprKind::Binary(op, l, r) => total(*op) && droppable(l) && droppable(r),
        // a constructor applied builds a value, and throws nothing
        ExprKind::App { .. } => applied(e).is_some_and(|(_, args)| args.into_iter().all(droppable)),
        _ => false,
    }
}

/// `C a b` as the constructor and its arguments, in order: pure stages
/// applied left to right.
pub fn applied(e: &Expr) -> Option<(&Constructor, Vec<&Expr>)> {
    let mut args = Vec::new();
    let mut head = e;
    while let ExprKind::App {
        func,
        arg,
        stage: Stage::Pure,
        arg_first: false,
    } = &head.kind
    {
        args.push(&**arg);
        head = func;
    }
    let ExprKind::Con(c) = &head.kind else {
        return None;
    };
    args.reverse();
    Some((c, args))
}

/// Whether an operator never throws, whatever its well-typed operands: a
/// negative power of a `BigInt` does.
pub fn total(op: BinOp) -> bool {
    op != BinOp::Arith(Arith::Pow, Num::BigInt)
}

/// Gives every local `e` binds a name ending in `suffix`, and renames its
/// reads to match: a copy of a body inlined next to the body it came from
/// keeps the locals of each definition unique.
pub fn rename(e: &mut Expr, suffix: &str) {
    let mut names = HashMap::new();
    binders(e, &mut names, suffix);
    apply(e, &names);
}

fn fresh(names: &mut HashMap<String, String>, name: &str, suffix: &str) {
    names.insert(name.to_owned(), format!("{name}{suffix}"));
}

fn pat_binders(p: &Pat, names: &mut HashMap<String, String>, suffix: &str) {
    let mut bound = Vec::new();
    p.binders(&mut bound);
    for b in bound {
        fresh(names, b, suffix);
    }
}

fn binders(e: &Expr, names: &mut HashMap<String, String>, suffix: &str) {
    match &e.kind {
        ExprKind::Lambda { params, .. } => {
            for p in params {
                pat_binders(&p.pat, names, suffix);
            }
        }
        ExprKind::Block(stmts, _) => {
            for s in stmts {
                match s {
                    Stmt::Let(p, _) => pat_binders(p, names, suffix),
                    Stmt::Fun { name, .. } => fresh(names, name, suffix),
                    Stmt::Expr(_) => {}
                }
            }
        }
        ExprKind::Match(_, arms) | ExprKind::Try(_, arms) => {
            for arm in arms {
                pat_binders(&arm.pat, names, suffix);
            }
        }
        ExprKind::Each { pat, .. } => pat_binders(pat, names, suffix),
        ExprKind::Scope { owner, outer, .. } => {
            for name in owner.iter().chain(outer) {
                fresh(names, name, suffix);
            }
        }
        _ => {}
    }
    e.each_child(&mut |c| binders(c, names, suffix));
}

fn apply(e: &mut Expr, names: &HashMap<String, String>) {
    let new = |n: &mut String| {
        if let Some(to) = names.get(n.as_str()) {
            n.clone_from(to);
        }
    };
    match &mut e.kind {
        ExprKind::Local(n) | ExprKind::Within(n, _) => new(n),
        ExprKind::Lambda { params, .. } => {
            for p in params {
                apply_pat(&mut p.pat, names);
            }
        }
        ExprKind::Block(stmts, _) => {
            for s in stmts {
                match s {
                    Stmt::Let(p, _) => apply_pat(p, names),
                    Stmt::Fun { name, .. } => new(name),
                    Stmt::Expr(_) => {}
                }
            }
        }
        ExprKind::Match(_, arms) | ExprKind::Try(_, arms) => {
            for arm in arms {
                apply_pat(&mut arm.pat, names);
            }
        }
        ExprKind::Each { pat, .. } => apply_pat(pat, names),
        ExprKind::Scope { owner, outer, .. } => {
            for n in owner.iter_mut().chain(outer.iter_mut()) {
                new(n);
            }
        }
        _ => {}
    }
    e.each_child_mut(&mut |c| apply(c, names));
}

fn apply_pat(p: &mut Pat, names: &HashMap<String, String>) {
    match p {
        Pat::Bind(n) => {
            if let Some(to) = names.get(n.as_str()) {
                n.clone_from(to);
            }
        }
        Pat::Wild | Pat::Lit(_) => {}
        Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
            for i in items {
                apply_pat(i, names);
            }
        }
        Pat::List(items, rest) => {
            for i in items.iter_mut().chain(rest.as_deref_mut()) {
                apply_pat(i, names);
            }
        }
        Pat::Record(fields) => {
            for (_, f) in fields {
                apply_pat(f, names);
            }
        }
    }
}
