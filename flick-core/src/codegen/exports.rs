//! Export behavior shared by JavaScript and TypeScript output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::names;
use flick_ir::Namespace;

/// The `export … from` lines of what a module exports of other modules,
/// `(module, namespace, name)`, by the module that defines each, and those
/// modules; a type that a value of its module is named after goes with it.
pub(crate) fn reexports(
    entities: &[(&str, Namespace, &str)],
    path: &dyn Fn(&str) -> String,
) -> (String, Vec<String>) {
    let mut from: BTreeMap<&str, (BTreeSet<String>, BTreeSet<&str>)> = BTreeMap::new();
    for &(module, namespace, name) in entities {
        let names = from.entry(module).or_default();
        match namespace {
            Namespace::Value | Namespace::Constructor => {
                names.0.insert(names::ident(name));
            }
            Namespace::Type => {
                names.1.insert(name);
            }
        }
    }
    let mut out = String::new();
    let mut modules = Vec::new();
    for (module, (values, types)) in from {
        let values: Vec<String> = values.into_iter().collect();
        let types: Vec<&str> = types
            .into_iter()
            .filter(|ty| !values.iter().any(|v| v == ty))
            .collect();
        if values.is_empty() && types.is_empty() {
            continue;
        }
        let specifier = names::string(&path(module));
        if !values.is_empty() {
            let _ = writeln!(out, "export {{ {} }} from {specifier};", values.join(", "));
        }
        if !types.is_empty() {
            let _ = writeln!(
                out,
                "export type {{ {} }} from {specifier};",
                types.join(", ")
            );
        }
        modules.push(module.to_owned());
    }
    (out, modules)
}
