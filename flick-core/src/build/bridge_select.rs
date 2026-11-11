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
            item.each_body(&mut |e| globals(e, &mut pending));
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
        item.each_body(&mut |e| globals(e, &mut pending));
    }
    available
}

fn needs_host(item: &Item) -> bool {
    let ItemKind::Foreign(foreign) = &item.kind else {
        return false;
    };
    let Some(source) = foreign.binding.from.as_deref() else {
        return false;
    };
    if super::bindings::UI_REQUIRED
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
    fn explicit_action_spans_select_the_adapter_ui_context() {
        let module = lower_text("App", "module App exposing (run)\nrun : Unit ->{} Unit\nlet run () = Ui.Tracing.with_action_spans true (task { () })\n").expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::AvailableWithUi
        );
    }

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
    fn a_serializable_atom_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (count, draft, item)\n\
             let count = Atom.serializable (Codec.json Schema.number) (Atom.make 0)\n\
             draft : String -> Atom.Writable String\n\
             let draft = Atom.serializable_family (Codec.json Schema.string) (fun _ -> Atom.make \"\")\n\
             item : Atom.Atom (Atom.Async String none)\n\
             let item = Atom.serializable_task (Codec.json Schema.string) (Atom.task (task { \"x\" }))\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_route_and_its_links_keep_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            r"module App exposing (page, to)
type Page = Home | User Float
  deriving Eq, Show, Schema, Url
page : Atom.Writable Page
let page = Atom.route Page.url Home
to : Page -> List Ui -> Ui
let to target children = Ui.route_link { route = page; url = Page.url; to = target } children
",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_reexported_stream_renderer_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (stream)\nstream : String -> Ui -> Stream.Core.Stream {Async, Reactivity.Atom.Registry} none String\nlet stream = Ui.Server.stream\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
        assert_eq!(
            select_modules(once(&module), EffectBridge::Absent),
            EffectBridge::Absent
        );
    }
}
