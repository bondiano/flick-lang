//! One owner for opaque UI values throughout a build. Native UI stays free
//! of Effect until a reachable binding needs the adapter's UI implementation.
use std::collections::{HashMap, HashSet};

use super::builtin_modules_of;
use super::effects::{on_kernel_with_bridge, rt_specifier, ui_family};
use crate::codegen::EffectBridge;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Module};
use crate::project::Compilation;

pub(super) fn select(compilation: &Compilation, available: EffectBridge) -> EffectBridge {
    select_modules(
        compilation.modules.iter().map(|module| &module.ir),
        available,
    )
}

fn select_modules<'a>(
    modules: impl Iterator<Item = &'a Module>,
    available: EffectBridge,
) -> EffectBridge {
    if available != EffectBridge::Available {
        return available;
    }
    let mut pending = Vec::new();
    // Include all exports of source libraries as well as the entry modules:
    // opaque atoms passed between them must use the same registry ABI.
    for module in modules {
        for item in &module.items {
            references(item, &mut pending);
            if needs_host(item) {
                return EffectBridge::AvailableWithUi;
            }
        }
    }
    let builtins = builtin_modules_of(true);
    let mut definitions = HashMap::new();
    for module in builtins {
        for item in &module.items {
            let name = match &item.kind {
                ItemKind::Value { name, .. } | ItemKind::Service { name, .. } => name,
                ItemKind::Foreign(foreign) => &foreign.name,
                _ => continue,
            };
            definitions.insert((module.name.clone(), name.clone()), item);
        }
    }
    let mut seen = HashSet::new();
    while let Some(key) = pending.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some(item) = definitions.get(&key) else {
            continue;
        };
        if needs_host(item) {
            return EffectBridge::AvailableWithUi;
        }
        references(item, &mut pending);
    }
    available
}

fn needs_host(item: &Item) -> bool {
    let ItemKind::Foreign(foreign) = &item.kind else {
        return false;
    };
    let Some(spec) = foreign.binding.from.as_deref().and_then(rt_specifier) else {
        return false;
    };
    ui_family(&spec, &foreign.binding.name)
        && !on_kernel_with_bridge(&spec, &foreign.binding.name, true)
}

fn references(item: &Item, pending: &mut Vec<(String, String)>) {
    let mut walk = |expr: &Expr| globals(expr, pending);
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => walk(body),
        ItemKind::Refined { check, .. } => walk(check),
        ItemKind::Instance { methods, .. } => methods.iter().for_each(|(_, method)| walk(method)),
        ItemKind::Foreign(foreign) => {
            if let Some(catch) = &foreign.catch {
                walk(catch);
            }
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
    }
}

fn globals(expr: &Expr, pending: &mut Vec<(String, String)>) {
    if let ExprKind::Global { module, name } = &expr.kind {
        pending.push((module.clone(), name.clone()));
    }
    expr.each_child(&mut |child| globals(child, pending));
}

#[cfg(test)]
mod tests {
    use super::select_modules;
    use crate::codegen::EffectBridge;
    use crate::ir::tests::lower_text;
    use std::iter::once;

    #[test]
    fn a_native_atom_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (state)\nlet state = Atom.make 0\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_reexported_stream_renderer_selects_one_adapter_ui_family() {
        let module = lower_text(
            "App",
            "module App exposing (stream)\nstream : String -> Ui -> Stream.Core.Stream {Async, Reactivity.Atom.Registry} none String\nlet stream = Ui.Server.stream\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::AvailableWithUi
        );
        assert_eq!(
            select_modules(once(&module), EffectBridge::Absent),
            EffectBridge::Absent
        );
    }
}
