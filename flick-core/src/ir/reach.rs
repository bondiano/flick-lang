//! What a program uses: the top-level items reachable from its `main`,
//! across modules. Backends and build validation share this graph, so a
//! built-in module may hold bindings a program never uses.

use std::collections::HashMap;
#[cfg(feature = "native-project")]
use std::collections::HashSet;

use crate::ir::{Expr, ExprKind, Item, ItemKind, Module};
use crate::types::{Dict, companion_name, default_method};

/// A top-level name: its module, and the value, foreign or instance name.
pub type Key = (String, String);

/// The names a top-level item defines.
pub fn defined(item: &Item) -> Vec<String> {
    match &item.kind {
        ItemKind::Value { name, .. } | ItemKind::Service { name, .. } => vec![name.clone()],
        ItemKind::Refined { name, .. } => ["check", "parse", "value"]
            .iter()
            .map(|m| companion_name(name, m))
            .collect(),
        ItemKind::Foreign(f) => vec![f.name.clone()],
        ItemKind::Instance { info, .. } => vec![info.name.clone()],
        ItemKind::Destructure { pattern, .. } => {
            let mut out = Vec::new();
            pattern.binders(&mut out);
            out.into_iter().map(str::to_owned).collect()
        }
        ItemKind::Constructor(_) => Vec::new(),
    }
}

/// The top-level names `item` refers to.
pub fn uses(item: &Item, out: &mut Vec<Key>) {
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => expr_uses(body, out),
        ItemKind::Instance { info, methods } => {
            for (_, d) in &info.supers {
                dict_uses(d, out);
            }
            if let Some((fields, _)) = &info.derived {
                for d in fields {
                    dict_uses(d, out);
                }
            }
            for (_, m) in methods {
                expr_uses(m, out);
            }
            let (module, class) = &info.class;
            for (name, _, extra) in &info.defaults {
                out.push((module.clone(), default_method(class, name)));
                for d in extra {
                    dict_uses(d, out);
                }
            }
        }
        ItemKind::Foreign(f) => {
            if let Some(c) = &f.catch {
                expr_uses(c, out);
            }
        }
        ItemKind::Refined { check, .. } => expr_uses(check, out),
        ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
    }
}

pub fn dict_uses(d: &Dict, out: &mut Vec<Key>) {
    match d {
        Dict::Instance { module, name, args } => {
            out.push((module.clone(), name.clone()));
            for a in args {
                dict_uses(a, out);
            }
        }
        Dict::Value { module, name } => out.push((module.clone(), name.clone())),
        Dict::Super { dict, .. } => dict_uses(dict, out),
        Dict::Structural { fields, .. } => {
            for f in fields {
                dict_uses(f, out);
            }
        }
        Dict::Opened { .. }
        | Dict::Given(_)
        | Dict::Field(_)
        | Dict::Primitive { .. }
        | Dict::Site { .. }
        | Dict::Error => {}
    }
}

fn expr_uses(e: &Expr, out: &mut Vec<Key>) {
    match &e.kind {
        ExprKind::Global { module, name } => out.push((module.clone(), name.clone())),
        ExprKind::Method { dict, .. } => dict_uses(dict, out),
        ExprKind::Dicts(_, dicts) => {
            for d in dicts {
                dict_uses(d, out);
            }
        }
        _ => {}
    }
    e.each_child(&mut |c| expr_uses(c, out));
}

/// Where each top-level name of `modules` is defined: module and item index.
pub fn index(modules: &[&Module]) -> HashMap<Key, (usize, usize)> {
    let mut out = HashMap::new();
    for (m, module) in modules.iter().enumerate() {
        for (i, item) in module.items.iter().enumerate() {
            for name in defined(item) {
                out.insert((module.name.clone(), name), (m, i));
            }
        }
    }
    out
}

/// The items reachable from `roots`, by module and item index, and the
/// modules reached that the program does not have.
#[cfg(feature = "native-project")]
pub fn reachable(
    modules: &[&Module],
    index: &HashMap<Key, (usize, usize)>,
    roots: &[Key],
) -> (HashSet<(usize, usize)>, Vec<String>) {
    let mut seen = HashSet::new();
    let mut missing = Vec::new();
    let mut pending: Vec<Key> = roots.to_vec();
    while let Some(key) = pending.pop() {
        let Some(&at) = index.get(&key) else {
            if !modules.iter().any(|m| m.name == key.0) && !missing.contains(&key.0) {
                missing.push(key.0);
            }
            continue;
        };
        if seen.insert(at) {
            uses(&modules[at.0].items[at.1], &mut pending);
        }
    }
    (seen, missing)
}
