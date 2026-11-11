//! The release optimizer: typed IR passes that keep every observable
//! contract of the debug lowering. Rules: `docs/src/content/docs/reference/codegen.md`.
//!
//! - [`devirt`]: a default an instance takes as its own method, the code of
//!   methods of known instances, and a `match` of a known constructor;
//! - [`specialize`]: a copy of a function of the module for the numeric
//!   instances it is called with, whose methods become operators;
//! - [`inline`]: known full applications of small pure functions of the
//!   module, and beta reduction;
//! - [`fold`]: literal expressions with exact JS semantics, constant branches;
//! - alias and constant propagation, and moving a pure value that cannot
//!   throw to its one use;
//! - [`fuse`]: `Seq` and `List` combinators of a `for` run in its loop;
//! - [`sroa`]: temporary records and tuples split into their parts;
//! - dead code: unused pure bindings and private top-level values.
//!
//! Code generation adds the rest in release: worker functions for known
//! full applications (`add$2`) and direct methods of canonical instances.
//! Nothing here reassociates arithmetic, folds `x * 0`, runs or memoizes a
//! task, merges layers, moves a service lookup across `provide` or drops a
//! runtime call: only pure code is rewritten.

mod devirt;
mod fold;
mod fuse;
mod inline;
mod specialize;
mod sroa;
mod walk;
pub use walk::rename;

#[cfg(test)]
mod gadt_tests;
#[cfg(test)]
mod inline_tests;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::mem;

use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Module, Pat, Stmt};
pub use fold::{js_number, json_string};
pub use specialize::names_only;
pub(crate) use walk::uses;
use walk::{droppable, each_strict_use, each_use};

/// Rounds of inlining: a body inlined in the first may inline again.
const ROUNDS: usize = 2;

/// The values of the other modules of the build, by module and name: their
/// dictionary parameters and body, when the body names only what its
/// module exports.
pub type Generics<'a> = &'a dyn Fn(&str, &str) -> Option<(Vec<String>, Expr)>;

/// Optimizes `module` in place; `exported` tells the values importers see,
/// `generics` what [`specialize`] may copy and [`inline`] may inline from
/// other modules.
pub fn optimize(module: &mut Module, exported: &dyn Fn(&str) -> bool, generics: Generics) {
    let mut fresh = 0;
    let foreign = inline::Foreign::new(generics);
    devirt::defaults(module, &devirt::Lookup::new(module, generics), &mut fresh);
    specialize::specialize(module, generics);
    for _ in 0..ROUNDS {
        constants(module);
        let lookup = devirt::Lookup::new(module, generics);
        let candidates = inline::candidates(&module.name, &module.items, exported);
        for item in &mut module.items {
            for body in bodies(item) {
                devirt::methods(body, &lookup, &mut fresh, 0);
                inline::inline(body, &module.name, &candidates, &foreign, &mut fresh);
                fuse::fuse(body, &mut fresh);
                simplify(body);
            }
        }
    }
    unused_items(module, exported);
}

fn bodies(item: &mut Item) -> Vec<&mut Expr> {
    match &mut item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => vec![body],
        ItemKind::Refined { check, .. } => vec![check],
        ItemKind::Instance { methods, .. } => methods.iter_mut().map(|(_, m)| m).collect(),
        ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => Vec::new(),
    }
}

/// Simplifies `e` until nothing changes, a few times at most.
pub fn simplify(e: &mut Expr) {
    for _ in 0..8 {
        if !pass(e) {
            return;
        }
    }
}

fn pass(e: &mut Expr) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= pass(c));
    changed |= specialize::operator(e) | fold::fold(e) | inline::beta(e) | devirt::known_match(e);
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        changed |= devirt::known_lets(stmts);
        changed |= flatten(stmts, result);
        changed |= sroa::split(stmts, result);
        changed |= propagate(stmts, result);
        changed |= dead(stmts, result);
        if stmts.is_empty() {
            let result = mem::replace(&mut **result, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
            *e = result;
            changed = true;
        }
    }
    changed
}

/// Hoists the statements of blocks a `let` binds or the block ends with:
/// locals are unique in a definition, so nothing is captured differently.
fn flatten(stmts: &mut Vec<Stmt>, result: &mut Box<Expr>) -> bool {
    let nested = |s: &Stmt| matches!(s, Stmt::Let(_, v) | Stmt::Var(_, v) | Stmt::Assign(_, v) | Stmt::Expr(v) if matches!(v.kind, ExprKind::Block(..)));
    if !stmts.iter().any(nested) && !matches!(result.kind, ExprKind::Block(..)) {
        return false;
    }
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in mem::take(stmts) {
        match stmt {
            Stmt::Let(
                p,
                Expr {
                    kind: ExprKind::Block(inner, value),
                    ..
                },
            ) => {
                out.extend(inner);
                out.push(Stmt::Let(p, *value));
            }
            Stmt::Expr(Expr {
                kind: ExprKind::Block(inner, value),
                ..
            }) => {
                out.extend(inner);
                out.push(Stmt::Expr(*value));
            }
            other => out.push(other),
        }
    }
    while let ExprKind::Block(inner, value) = &mut result.kind {
        out.append(inner);
        let value = mem::replace(
            &mut **value,
            Expr::new(ExprKind::Lit(Lit::Unit), result.span),
        );
        **result = value;
    }
    *stmts = out;
    true
}

/// A value whose copies are as cheap as the binding: a literal, a name.
fn atom(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(Lit::String(s)) => s.len() <= 32,
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Global { .. } | ExprKind::Con(_) => true,
        ExprKind::Neg(_, inner) => matches!(inner.kind, ExprKind::Lit(_)),
        _ => false,
    }
}

/// For each statement that binds one local with `let`: how often the
/// statements after it and `result` read the local, and how often they
/// surely evaluate that read once. One walk of the block from its end.
fn later_uses(stmts: &[Stmt], result: &Expr) -> Vec<Option<(usize, usize)>> {
    let mut all: HashMap<&str, usize> = HashMap::new();
    let mut strict: HashMap<&str, usize> = HashMap::new();
    each_use(result, &mut |l| *all.entry(l).or_default() += 1);
    each_strict_use(result, &mut |l| *strict.entry(l).or_default() += 1);
    let mut later = vec![None; stmts.len()];
    for (i, stmt) in stmts.iter().enumerate().rev() {
        if let Stmt::Let(Pat::Bind(name), _) = stmt {
            let of = |counts: &HashMap<&str, usize>| counts.get(name.as_str()).copied();
            later[i] = Some((of(&all).unwrap_or(0), of(&strict).unwrap_or(0)));
        }
        match stmt {
            Stmt::Let(_, v) | Stmt::Var(_, v) | Stmt::Assign(_, v) | Stmt::Expr(v) => {
                each_use(v, &mut |l| *all.entry(l).or_default() += 1);
                each_strict_use(v, &mut |l| *strict.entry(l).or_default() += 1);
            }
            Stmt::Fun { value, .. } => each_use(value, &mut |l| *all.entry(l).or_default() += 1),
        }
    }
    later
}

/// Replaces a local bound to an atom with the atom, and a local read once
/// by its pure value when that read surely happens. One walk of the block:
/// a value on its way to its reads waits in `moving`, and each statement
/// takes what it reads when the walk comes to it. A value reads no local
/// bound after it, so the counts taken before the walk still hold then.
fn propagate(stmts: &mut Vec<Stmt>, result: &mut Expr) -> bool {
    let later = later_uses(stmts, result);
    let before = stmts.len();
    let mut moving: HashMap<String, Expr> = HashMap::new();
    let mut kept = Vec::with_capacity(before);
    for (mut stmt, uses) in mem::take(stmts).into_iter().zip(later) {
        // what moves in may be dropped, so a value that may be dropped
        // before it still may after: asking first keeps the answer from
        // walking the values that moved into this one
        let once = matches!(uses, Some((1, 1)));
        let pure = once && matches!(&stmt, Stmt::Let(_, v) if droppable(v));
        match &mut stmt {
            Stmt::Let(_, v)
            | Stmt::Var(_, v)
            | Stmt::Assign(_, v)
            | Stmt::Expr(v)
            | Stmt::Fun { value: v, .. } => place(v, &mut moving),
        }
        match (stmt, uses) {
            (Stmt::Let(Pat::Bind(name), value), Some((all, _)))
                if all != 0 && (atom(&value) || pure || (once && droppable(&value))) =>
            {
                moving.insert(name, value);
            }
            (stmt, _) => kept.push(stmt),
        }
    }
    place(result, &mut moving);
    *stmts = kept;
    stmts.len() != before
}

/// Puts the values of `moving` where `e` reads their locals: an atom is
/// copied to each read; any other value has one read, and moves there.
fn place(e: &mut Expr, moving: &mut HashMap<String, Expr>) {
    if moving.is_empty() {
        return;
    }
    if let ExprKind::Local(l) = &e.kind
        && let Some(value) = moving.get(l)
    {
        *e = if atom(value) {
            value.clone()
        } else {
            moving.remove(l).expect("found above")
        };
        return;
    }
    e.each_child_mut(&mut |c| place(c, moving));
}

/// Drops statements whose value is unused and whose evaluation shows
/// nothing; a runtime call always stays. One walk of the block from its
/// end, with the locals the statements kept so far and `result` read.
fn dead(stmts: &mut Vec<Stmt>, result: &Expr) -> bool {
    let keep: Vec<bool> = {
        let mut read: HashSet<&str> = HashSet::new();
        each_use(result, &mut |l| {
            read.insert(l);
        });
        let mut keep = vec![true; stmts.len()];
        for (i, stmt) in stmts.iter().enumerate().rev() {
            let drop = match stmt {
                Stmt::Expr(v) => droppable(v),
                Stmt::Let(p, v) => {
                    let mut bound = Vec::new();
                    p.binders(&mut bound);
                    p.irrefutable() && bound.iter().all(|n| !read.contains(n)) && droppable(v)
                }
                Stmt::Fun { name, .. } => !read.contains(name.as_str()),
                Stmt::Var(..) | Stmt::Assign(..) => false,
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
                | Stmt::Fun { value: v, .. } => {
                    each_use(v, &mut |l| {
                        read.insert(l);
                    });
                }
            }
        }
        keep
    };
    let before = stmts.len();
    let mut keep = keep.into_iter();
    stmts.retain(|_| keep.next().expect("one for each statement"));
    stmts.len() != before
}

/// Replaces the uses of a top-level value of the module bound to a literal
/// with the literal, as `ReScript` does; `unused_items` drops it if private.
fn constants(module: &mut Module) {
    let literals: Vec<(String, Expr)> = module
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, body }
                if dicts.is_empty() && atom(body) && fold::constant(body).is_some() =>
            {
                Some((name.clone(), body.clone()))
            }
            _ => None,
        })
        .collect();
    if literals.is_empty() {
        return;
    }
    for item in &mut module.items {
        for body in bodies(item) {
            replace_globals(body, &module.name, &literals);
        }
    }
}

fn replace_globals(e: &mut Expr, module: &str, literals: &[(String, Expr)]) {
    if let ExprKind::Global { module: m, name } = &e.kind
        && m == module
        && let Some((_, value)) = literals.iter().find(|(n, _)| n == name)
    {
        *e = Expr::new(value.kind.clone(), e.span);
        return;
    }
    e.each_child_mut(&mut |c| replace_globals(c, module, literals));
}

/// Drops private top-level values nothing reaches whose definition shows
/// nothing when the module loads.
fn unused_items(module: &mut Module, exported: &dyn Fn(&str) -> bool) {
    loop {
        let mut used = BTreeSet::new();
        for item in &module.items {
            item.each_body(&mut |e| globals(e, &module.name, &mut used));
        }
        let before = module.items.len();
        module.items.retain(|item| match &item.kind {
            ItemKind::Value { name, dicts, body } => {
                exported(name)
                    || name.contains('$')
                    || used.contains(name)
                    || !(dicts.is_empty() && droppable(body))
            }
            _ => true,
        });
        if module.items.len() == before {
            return;
        }
    }
}

fn globals(e: &Expr, module: &str, out: &mut BTreeSet<String>) {
    if let ExprKind::Global { module: m, name } = &e.kind
        && m == module
    {
        out.insert(name.clone());
    }
    e.each_child(&mut |c| globals(c, module, out));
}
