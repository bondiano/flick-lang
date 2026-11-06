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

#[cfg(test)]
mod gadt_tests;
#[cfg(test)]
mod inline_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::mem;

use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Module, Pat, Stmt};
pub use fold::{js_number, json_string};
pub use specialize::names_only;
pub(crate) use walk::uses;
use walk::{droppable, strict_uses, substitute};

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
    let nested = |s: &Stmt| matches!(s, Stmt::Let(_, v) | Stmt::Expr(v) if matches!(v.kind, ExprKind::Block(..)));
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

/// How often the statements after `i` and `result` read `name`; and how
/// often they surely evaluate the read once.
fn later_uses(stmts: &[Stmt], i: usize, result: &Expr, name: &str) -> (usize, usize) {
    let mut all = uses(result, name);
    let mut strict = strict_uses(result, name);
    for s in &stmts[i + 1..] {
        match s {
            Stmt::Let(_, v) | Stmt::Expr(v) => {
                all += uses(v, name);
                strict += strict_uses(v, name);
            }
            Stmt::Fun { value, .. } => all += uses(value, name),
        }
    }
    (all, strict)
}

/// Replaces a local bound to an atom with the atom, and a local read once
/// by its pure value when that read surely happens.
fn propagate(stmts: &mut Vec<Stmt>, result: &mut Expr) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let Stmt::Let(Pat::Bind(name), value) = &stmts[i] else {
            i += 1;
            continue;
        };
        let (all, strict) = later_uses(stmts, i, result, name);
        let movable = atom(value) || (droppable(value) && all == 1 && strict == 1);
        if !movable || all == 0 {
            i += 1;
            continue;
        }
        let Stmt::Let(Pat::Bind(name), value) = stmts.remove(i) else {
            unreachable!("matched above")
        };
        for s in &mut stmts[i..] {
            match s {
                Stmt::Let(_, v) | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => {
                    substitute(v, &name, &value);
                }
            }
        }
        substitute(result, &name, &value);
        changed = true;
    }
    changed
}

/// Drops statements whose value is unused and whose evaluation shows
/// nothing; a runtime call always stays.
fn dead(stmts: &mut Vec<Stmt>, result: &Expr) -> bool {
    let before = stmts.len();
    let mut i = stmts.len();
    while i > 0 {
        i -= 1;
        let unused = |names: &[&str]| names.iter().all(|n| later_uses(stmts, i, result, n).0 == 0);
        let drop = match &stmts[i] {
            Stmt::Expr(v) => droppable(v),
            Stmt::Let(p, v) => {
                let mut bound = Vec::new();
                p.binders(&mut bound);
                p.irrefutable() && unused(&bound) && droppable(v)
            }
            Stmt::Fun { name, .. } => unused(&[name.as_str()]),
        };
        if drop {
            stmts.remove(i);
        }
    }
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
            item_globals(item, &module.name, &mut used);
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

fn item_globals(item: &Item, module: &str, out: &mut BTreeSet<String>) {
    let mut walk = |e: &Expr| globals(e, module, out);
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => walk(body),
        ItemKind::Refined { check, .. } => walk(check),
        ItemKind::Instance { methods, .. } => methods.iter().for_each(|(_, m)| walk(m)),
        ItemKind::Foreign(f) => {
            if let Some(c) = &f.catch {
                walk(c);
            }
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
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
