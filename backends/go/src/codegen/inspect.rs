//! Single-module Go codegen for `flick inspect --emit output --target go`.

use std::collections::{BTreeSet, HashMap};

use flick_backend_support::reach::{self, Key};
use flick_ir::Module;
use flick_pretty::DocumentArena;

use super::main_file::item_order;
use super::{
    Gen, Input, Output, Unsupported, adt_derive, dedup, direct, go_type, names, own, shape,
};

/// Compiles one module for `flick inspect --emit output --target go`, without
/// requiring a `main` entry or running `go build`.
pub fn go_module(input: &Input, module_name: &str) -> Output {
    let modules: Vec<&Module> = input.own.iter().chain(input.builtins).copied().collect();
    let modules = &modules[..];
    let decls = go_type::type_decls(input.own, input.builtins);
    let index = reach::index(modules);
    let Some((m, module)) = modules
        .iter()
        .enumerate()
        .find(|(_, m)| m.name == module_name)
    else {
        return Output {
            files: Vec::new(),
            diagnostics: vec![Unsupported::new(format!("unknown module `{module_name}`"))],
            packages: Vec::new(),
            go_imports: BTreeSet::new(),
        };
    };
    let keys: Vec<Key> = module
        .items
        .iter()
        .flat_map(|item| {
            reach::defined(item)
                .into_iter()
                .map(|name| (module.name.clone(), name))
        })
        .collect();
    let (mut reached, missing) = reach::reachable(modules, &index, &keys);
    for i in 0..module.items.len() {
        reached.insert((m, i));
    }
    let mut diagnostics: Vec<Unsupported> = missing
        .into_iter()
        .map(|module| {
            Unsupported::new(format!(
                "the module `{module}` of a package is not available on the go target yet"
            ))
        })
        .collect();
    let mut shapes = HashMap::new();
    for &(mi, ii) in &reached {
        let item = &modules[mi].items[ii];
        for name in reach::defined(item) {
            let held = shape::shape(input, &decls, &modules[mi].name, &item.kind);
            shapes.insert((modules[mi].name.clone(), name), held);
        }
        shapes.extend(direct::shapes(input, &decls, &modules[mi].name, &item.kind));
    }
    let reached_items = reached
        .iter()
        .map(|&(mi, ii)| (&*modules[mi].name, &modules[mi].items[ii]));
    let settled = shape::settled(reached_items, &shapes);
    let derived = adt_derive::derived_instances(modules, input.lang);
    let item_initializers = modules.iter().any(|m| !m.initialization.is_empty());
    let records = own::records(modules, input.lang, &decls);
    let fields = own::fields(input.own, input.lang);
    let items: Vec<usize> = (0..module.items.len()).collect();
    let arena = DocumentArena::new();
    let mut g = Gen::new(&module.name, &shapes, input, &arena);
    g.settled = Some(&settled);
    g.records = Some(&records);
    g.fields = Some(&fields);
    g.decls = Some(&decls);
    g.derived = Some(&derived);
    g.item_initializers = item_initializers;
    let order = item_order(module, &items, &index);
    let code = g.module(module, &order);
    diagnostics.extend(g.errors.into_iter().map(|(message, span)| Unsupported {
        message,
        span: span.filter(|_| m < input.own.len()),
    }));
    let go_imports = g.go_imports.into_keys().collect();
    dedup(&mut diagnostics);
    Output {
        files: vec![(names::file(&module.name), code)],
        diagnostics,
        packages: Vec::new(),
        go_imports,
    }
}
