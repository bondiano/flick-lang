//! Export behavior shared by JavaScript and TypeScript output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::names;
use crate::iface::{Decl, Export, Interface, Namespace};
use crate::types::{Type, builtin_sources};

/// Re-exports from the modules that define them, without pulling unused
/// built-in facade members into a program.
pub(crate) fn reexports(
    module: &str,
    interface: &Interface,
    path: &dyn Fn(&str) -> String,
    value: &dyn Fn(&Export) -> bool,
    ty: &dyn Fn(&Export) -> bool,
) -> (String, Vec<String>) {
    if builtin_sources().iter().any(|(name, _)| *name == module) {
        return (String::new(), Vec::new());
    }
    let mut from: BTreeMap<&str, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for export in &interface.exports {
        let entity = &export.entity;
        if entity.module == module {
            continue;
        }
        let names = from.entry(&entity.module).or_default();
        match entity.namespace {
            Namespace::Value | Namespace::Constructor
                if !matches!(export.decl, Decl::Method | Decl::Blocked) && value(export) =>
            {
                names.0.insert(names::ident(&entity.name));
            }
            Namespace::Type if ty(export) => {
                names.1.insert(entity.name.clone());
            }
            _ => {}
        }
    }
    let mut out = String::new();
    let mut modules = Vec::new();
    for (module, (values, types)) in from {
        let values: Vec<String> = values.into_iter().collect();
        let types: Vec<&String> = types.iter().filter(|ty| !values.contains(ty)).collect();
        if values.is_empty() && types.is_empty() {
            continue;
        }
        let specifier = names::string(&path(module));
        if !values.is_empty() {
            let _ = writeln!(out, "export {{ {} }} from {specifier};", values.join(", "));
        }
        if !types.is_empty() {
            let list: Vec<&str> = types.iter().map(|ty| ty.as_str()).collect();
            let list = list.join(", ");
            let _ = writeln!(out, "export type {{ {list} }} from {specifier};");
        }
        modules.push(module.to_owned());
    }
    (out, modules)
}

/// The supplied arrows of an exported value whose last stage is Sync.
pub(crate) fn sync_stages(scheme: &Type) -> Option<usize> {
    let mut ty = scheme;
    let mut stages = 0;
    loop {
        match ty {
            Type::Forall { body, .. } => ty = body,
            Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } => ty = inner,
            Type::Fun {
                effects, result, ..
            } => {
                stages += 1;
                match effects {
                    None => ty = result,
                    Some(effects) => return effects.sync.then_some(stages),
                }
            }
            _ => return None,
        }
    }
}
