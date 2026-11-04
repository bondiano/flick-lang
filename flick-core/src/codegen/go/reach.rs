//! What a program uses: the top-level items reachable from its `main`,
//! across modules. Only those are compiled, so a built-in module may hold
//! what the target cannot run as long as the program does not reach it.

use std::collections::{HashMap, HashSet};

use crate::ir::{Expr, ExprKind, Item, ItemKind, Module};
use crate::types::{Dict, SchemaPlan, companion_name, default_method};

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
        ItemKind::Schema(s) => {
            let mut out = vec![companion_name(&s.ty, "schema")];
            if matches!(s.plan, SchemaPlan::Refined { .. }) {
                out.push(companion_name(&s.ty, "json"));
            }
            out
        }
        ItemKind::Constructor(_) => Vec::new(),
    }
}

/// The top-level names `item` refers to.
pub fn uses(module: &str, item: &Item, out: &mut Vec<Key>) {
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
            for (name, _) in &info.defaults {
                out.push((module.clone(), default_method(class, name)));
            }
        }
        ItemKind::Foreign(f) => {
            if let Some(c) = &f.catch {
                expr_uses(c, out);
            }
        }
        ItemKind::Refined { check, .. } => expr_uses(check, out),
        ItemKind::Schema(s) => plan_uses(module, &s.plan, out),
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
        Dict::Super { dict, .. } => dict_uses(dict, out),
        Dict::Structural { fields, .. } => {
            for f in fields {
                dict_uses(f, out);
            }
        }
        Dict::Given(_) | Dict::Primitive { .. } | Dict::Error => {}
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
            uses(
                &modules[at.0].name,
                &modules[at.0].items[at.1],
                &mut pending,
            );
        }
    }
    (seen, missing)
}

/// The schemas of other types a schema plan is built of.
fn plan_uses(module: &str, plan: &SchemaPlan, out: &mut Vec<Key>) {
    match plan {
        SchemaPlan::List(p)
        | SchemaPlan::Option(p)
        | SchemaPlan::Redacted(p)
        | SchemaPlan::Applied(_, p) => plan_uses(module, p, out),
        SchemaPlan::Result(a, b) => {
            plan_uses(module, a, out);
            plan_uses(module, b, out);
        }
        SchemaPlan::Tuple(items) => items.iter().for_each(|p| plan_uses(module, p, out)),
        SchemaPlan::Record(fields) => fields.iter().for_each(|(_, p)| plan_uses(module, p, out)),
        SchemaPlan::Adt { variants, .. } => {
            for p in variants.iter().flat_map(|(_, fields)| fields) {
                plan_uses(module, p, out);
            }
        }
        SchemaPlan::Named {
            module, ty, args, ..
        } => {
            out.push((module.clone(), companion_name(ty, "schema")));
            for p in args {
                plan_uses(module, p, out);
            }
        }
        SchemaPlan::Refined { name, base, .. } => {
            out.push((module.to_owned(), companion_name(name, "check")));
            plan_uses(module, base, out);
        }
        SchemaPlan::Prim(_) | SchemaPlan::Param(_) | SchemaPlan::Constructor(_) => {}
    }
}
