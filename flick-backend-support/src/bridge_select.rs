//! One owner for opaque UI and web values throughout a build. Native UI stays
//! free of Effect until a reachable binding needs the adapter's implementation.

use std::collections::{HashMap, HashSet};

use flick_ir::{Expr, ExprKind, Item, ItemKind, Module};

use crate::EffectBridge;
use crate::effects::{bindings, on_kernel_with_bridge, rt_specifier, ui_family, web_family};

/// Choose the [`EffectBridge`] ownership level for `modules` given `builtins`,
/// `available` (whether the adapter package is declared/available), and extra
/// entry-runner `(module, name)` roots.
pub fn select<'a>(
    modules: impl Iterator<Item = &'a Module>,
    builtins: impl Iterator<Item = &'a Module>,
    available: EffectBridge,
    mut pending: Vec<(String, String)>,
) -> EffectBridge {
    if available != EffectBridge::Available {
        return available;
    }
    let (mut ui, mut web) = (false, false);
    let mut owned = |item: &Item| {
        ui |= needs_host(item);
        web |= needs_web_adapter(item);
        ui && web
    };
    // Include all exports of source libraries as well as the entry modules:
    // opaque atoms passed between them must use the same registry ABI.
    for module in modules {
        for item in &module.items {
            item.each_body(&mut |e| globals(e, &mut pending));
            if owned(item) {
                return EffectBridge::owning(true, true);
            }
        }
    }
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
        if owned(item) {
            return EffectBridge::owning(true, true);
        }
        item.each_body(&mut |e| globals(e, &mut pending));
    }
    EffectBridge::owning(ui, web)
}

/// A reachable web binding only the adapter has: an Http one, an RPC server,
/// a stream or a transport of its own. Requests, responses and procedures
/// then cross to it as its values.
fn needs_web_adapter(item: &Item) -> bool {
    let ItemKind::Foreign(foreign) = &item.kind else {
        return false;
    };
    let Some(source) = foreign.binding.from.as_deref() else {
        return false;
    };
    web_family(source)
        && !rt_specifier(source)
            .is_some_and(|spec| on_kernel_with_bridge(&spec, &foreign.binding.name, true))
}

fn needs_host(item: &Item) -> bool {
    let ItemKind::Foreign(foreign) = &item.kind else {
        return false;
    };
    let Some(source) = foreign.binding.from.as_deref() else {
        return false;
    };
    if bindings::UI_REQUIRED
        .iter()
        .any(|line| line.split_once(' ') == Some((source, foreign.binding.name.as_str())))
    {
        return true;
    }
    let Some(spec) = rt_specifier(source) else {
        return false;
    };
    ui_family(&spec, &foreign.binding.name)
        && !on_kernel_with_bridge(&spec, &foreign.binding.name, true)
}

fn globals(expr: &Expr, pending: &mut Vec<(String, String)>) {
    if let ExprKind::Global { module, name, .. } = &expr.kind {
        pending.push((module.clone(), name.clone()));
    }
    expr.each_child(&mut |child| globals(child, pending));
}
