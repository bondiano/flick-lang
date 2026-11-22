//! The release optimizer: typed IR passes that keep every observable
//! contract of the debug lowering. Rules: `docs/src/content/docs/reference/codegen.md`.
//!
//! - [`devirt`]: a default an instance takes as its own method, the code of
//!   methods of known instances, and a `match` of a known constructor;
//! - [`specialize`]: a copy of a function of the module for the numeric
//!   instances it is called with, whose methods become operators;
//! - [`inline`]: known full applications of small pure functions of the
//!   module, and beta reduction;
//! - [`known`]: a function that chooses what it gives is inlined where that
//!   spares an object, and a `match` goes to where its data is known, so a
//!   value built only to be taken apart is not built;
//! - [`fold`]: literal expressions with exact JS semantics, constant branches;
//! - alias and constant propagation, and moving a pure value that cannot
//!   throw to its one use;
//! - [`fuse`]: `Seq` and `List` combinators of a `for` run in its loop;
//! - [`lazy`]: lazy cells a program need not make: one nothing reads, one of
//!   a value, made done, and one forced where it is made; a force of a cell an
//!   earlier force left done; and a pure, loop-invariant lazy hoisted out of a
//!   loop;
//! - [`sroa`]: temporary records and tuples split into their parts;
//! - dead code: unused pure bindings and private top-level values.
//!
//! Code generation adds the rest in release: worker functions for known
//! full applications (`add$2`) and direct methods of canonical instances.
//! Nothing here reassociates arithmetic, folds `x * 0`, runs or memoizes a
//! task, merges layers, moves a service lookup across `provide` or drops a
//! runtime call: only pure code is rewritten, and the lazy cells of [`lazy`],
//! each a contextual equivalence of the core.

pub mod devirt;
pub mod eval;
mod fold;
mod fuse;
mod inline;
mod known;
mod lazy;
mod specialize;
mod sroa;
pub mod tuple_match;
mod walk;
pub use walk::{Arity, arity, rename};

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::{mem, ptr};

use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Module, Pat, Stmt};
pub use fold::{js_number, json_string};
pub use specialize::names_only;
use walk::{droppable, each_strict_use, each_use, inert, partial};

/// Rounds of inlining: a body inlined in the first may inline again.
const ROUNDS: usize = 2;

/// How many functions in a row are inlined into each other where that
/// spares an object, as a chain of combinators is.
const STEPS: usize = 8;

/// The arity known of a value of another module that is no function.
const NO_ARITY: usize = usize::MAX;

/// The values of the other modules of the build, by module and name: their
/// dictionary parameters and body, when the body names only what its
/// module exports.
pub type Generics<'a> = &'a dyn Fn(&str, &str) -> Option<(Vec<String>, Expr)>;

/// Optimizes `module` in place; `exported` tells the values importers see,
/// `generics` what [`specialize`] may copy and [`inline`] may inline from
/// other modules, `arity` how many arguments a function of another module
/// takes before its body runs.
pub fn optimize(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    generics: Generics,
    arity: Arity,
    lang: &crate::Known<'_>,
    builtin: &dyn Fn(&str) -> bool,
) {
    let config = eval::EvalConfig::default();
    let _ = optimize_with_eval(
        module,
        exported,
        generics,
        arity,
        lang,
        builtin,
        &config,
        &eval::NoPrimitives,
    );
}

/// [`optimize`], with explicit evaluator budgets and a target [`eval::PrimitiveEval`]
/// hook, returning stable internal outcome counters.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit optimizer configuration boundary"
)]
pub fn optimize_with_eval(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    generics: Generics,
    arity: Arity,
    lang: &crate::Known<'_>,
    builtin: &dyn Fn(&str) -> bool,
    eval_config: &eval::EvalConfig,
    primitives: &dyn eval::PrimitiveEval,
) -> eval::EvalCounters {
    let mut fresh = 0;
    let foreign = inline::Foreign::new(generics, builtin);
    // what other modules' functions take, by `module.name`: one map type
    // the compiler has already
    let found: RefCell<HashMap<String, usize>> = RefCell::default();
    let elsewhere = |m: &str, name: &str| {
        let key = format!("{m}.{name}");
        let known = found.borrow().get(&key).copied();
        let taken = known.unwrap_or_else(|| {
            let taken = arity(m, name).unwrap_or(NO_ARITY);
            found.borrow_mut().insert(key, taken);
            taken
        });
        (taken != NO_ARITY).then_some(taken)
    };
    devirt::defaults(
        module,
        &devirt::Lookup::new(module, generics, lang),
        &mut fresh,
    );
    specialize::specialize(module, generics, lang);
    let mut module_steps = eval_config.max_module_steps;
    let mut retries = eval::RetryTracker::default();
    let mut counters = eval::EvalCounters::default();
    for _ in 0..ROUNDS {
        constants(module, lang);
        let lookup = devirt::Lookup::new(module, generics, lang);
        let candidates = inline::candidates(&module.name, &module.items, exported);
        let mut own: HashMap<String, usize> = HashMap::new();
        for (name, n) in module.items.iter().filter_map(walk::arity) {
            own.insert(name.to_owned(), n);
        }
        let name = module.name.clone();
        let arities = |m: &str, n: &str| {
            if m == name {
                own.get(n).copied()
            } else {
                elsewhere(m, n)
            }
        };
        for item in &mut module.items {
            for body in bodies(item) {
                devirt::methods(body, &lookup, &mut fresh, 0);
                inline::inline(body, &module.name, &candidates, &foreign, &mut fresh);
                fuse::fuse(body, &mut fresh, lang);
                lazy::hoist(body, &mut fresh, lang);
                simplify_with(body, &arities, lang);
                // a value built only to be taken apart: each step may
                // show the next one
                for _ in 0..STEPS {
                    let inlined = inline::shaped(body, &foreign, &mut fresh, &mut Vec::new());
                    if !(inlined | known::branches(body, &mut fresh)) {
                        break;
                    }
                    simplify_with(body, &arities, lang);
                }
                if lazy::hoist(body, &mut fresh, lang) {
                    simplify_with(body, &arities, lang);
                }
            }
        }
        let eval_lookup = devirt::Lookup::new(module, generics, lang);
        if eval::run_module_pass(
            module,
            exported,
            &eval_lookup,
            generics,
            lang,
            primitives,
            eval_config,
            &mut module_steps,
            &mut retries,
            &mut counters,
        ) {
            for item in &mut module.items {
                for body in bodies(item) {
                    simplify_with(body, &arities, lang);
                }
            }
        }
    }
    unused_items(module, exported);
    counters
}

fn bodies(item: &mut Item) -> Vec<&mut Expr> {
    match &mut item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => vec![body],
        ItemKind::Instance { methods, .. } => methods.iter_mut().map(|(_, m)| m).collect(),
        ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => Vec::new(),
    }
}

/// Simplifies `e` until nothing changes, a few times at most.
pub fn simplify(e: &mut Expr, lang: &crate::Known<'_>) {
    simplify_with(e, &|_, _| None, lang);
}

/// [`simplify`], knowing how many arguments the functions `arity` names take.
fn simplify_with(e: &mut Expr, arity: Arity, lang: &crate::Known<'_>) {
    for _ in 0..8 {
        if !pass(e, arity, lang) {
            return;
        }
    }
}

fn pass(e: &mut Expr, arity: Arity, lang: &crate::Known<'_>) -> bool {
    if let ExprKind::WithOwnerEvidence(owner, call) = &mut e.kind {
        let mut changed = pass(owner, arity, lang);
        call.each_child_mut(&mut |child| changed |= pass(child, arity, lang));
        return changed;
    }
    // the owner is the call's: in another shape of it, the first call code
    // generation meets would take it, maybe one inside its argument
    if let ExprKind::Within(_, call) = &mut e.kind {
        let mut changed = false;
        call.each_child_mut(&mut |c| changed |= pass(c, arity, lang));
        return changed;
    }
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= pass(c, arity, lang));
    changed |= walk::ordered(e, arity);
    changed |= specialize::operator(e, lang)
        | fold::fold(e, lang)
        | inline::beta(e)
        | devirt::known_match(e)
        | lazy::cells(e, lang);
    changed |= known::named_again(e);
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        changed |= devirt::known_lets(stmts);
        changed |= known::scrutinees(stmts, result);
        changed |= lazy::unnamed(stmts, result, lang);
        changed |= lazy::forced_again(stmts, result, lang);
        changed |= flatten(stmts, result);
        changed |= sroa::split(stmts, result);
        changed |= propagate(stmts, result, arity);
        changed |= dead(stmts, result, arity);
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

include!("propagation.rs");

/// Replaces the uses of a top-level value of the module bound to a literal
/// with the literal, as `ReScript` does; `unused_items` drops it if private.
fn constants(module: &mut Module, lang: &crate::Known<'_>) {
    let literals: Vec<(String, Expr)> = module
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, body }
                if dicts.is_empty() && atom(body) && fold::constant(body, lang).is_some() =>
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

#[cfg(test)]
mod context_tests;
