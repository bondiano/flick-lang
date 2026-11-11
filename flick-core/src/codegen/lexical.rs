//! Runtime placement is separate from nominal ownership. A lexical prefix
//! has its own physical ESM file, while tags, services and types keep the
//! defining Flick module's identity.
use super::{Options, Output, names};
use crate::ir::{Item, ItemKind, Module, reach};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

#[derive(Default)]
pub(crate) struct RuntimeLayout {
    // Captured prefixes are sparse. Small vectors avoid a second family of
    // hash tables in the browser compiler and allocate only for actual owners.
    symbols: Vec<(String, Vec<(String, String)>)>,
}

fn defined(item: &Item) -> Vec<String> {
    if let ItemKind::Constructor(c) = &item.kind {
        vec![c.name.clone()]
    } else {
        reach::defined(item)
    }
}

pub(crate) fn prefix_key(module: &str, before: u32) -> String {
    format!("{module}$lexical${before}")
}

/// Optimization may introduce private helpers. They move with the lexical
/// binding that uses them, rather than acquiring a dependency on its facade.
pub(crate) fn phases(module: &Module) -> Vec<Option<u32>> {
    let mut phases = vec![None; module.items.len()];
    let index: HashMap<_, _> = module
        .items
        .iter()
        .enumerate()
        .flat_map(|(i, item)| defined(item).into_iter().map(move |name| (name, i)))
        .collect();
    for prefix in &module.initialization.prefixes {
        let mut pending: Vec<_> = prefix
            .bindings
            .iter()
            .filter_map(|name| index.get(name).copied())
            .collect();
        while let Some(i) = pending.pop() {
            if phases[i].is_some_and(|old| old <= prefix.before) {
                continue;
            }
            phases[i] = Some(prefix.before);
            let mut used = Vec::new();
            reach::uses(&module.items[i], &mut used);
            pending.extend(
                used.into_iter()
                    .filter(|(owner, _)| owner == &module.name)
                    .filter_map(|(_, name)| index.get(&name).copied()),
            );
        }
    }
    phases
}

impl RuntimeLayout {
    pub(crate) fn new<'a>(modules: impl Iterator<Item = &'a Module>) -> Self {
        let mut layout = Self::default();
        for module in modules {
            layout.add(module);
        }
        layout
    }
    fn add(&mut self, module: &Module) {
        if module.initialization.prefixes.is_empty() {
            return;
        }
        for (item, phase) in module.items.iter().zip(phases(module)) {
            let Some(phase) = phase else {
                continue;
            };
            for symbol in super::js::runtime_exports(item) {
                let index = self
                    .symbols
                    .iter()
                    .position(|(owner, _)| owner == &module.name)
                    .unwrap_or_else(|| {
                        self.symbols.push((module.name.clone(), Vec::new()));
                        self.symbols.len() - 1
                    });
                self.symbols[index]
                    .1
                    .push((symbol, prefix_key(&module.name, phase)));
            }
        }
    }
    fn location(&self, module: &str, symbol: &str) -> Option<String> {
        self.symbols
            .iter()
            .find(|(owner, _)| owner == module)?
            .1
            .iter()
            .find(|(name, _)| name == symbol)
            .map(|(_, location)| location.clone())
    }
}

pub(crate) fn js_parts(
    module: &Module,
    options: &Options,
    layout: &RuntimeLayout,
) -> Vec<(String, Output)> {
    if module.initialization.prefixes.is_empty() {
        return vec![(
            module.name.clone(),
            super::js::js_at(module, options, &module.name, &|owner, symbol| {
                layout.location(owner, symbol)
            }),
        )];
    }
    let mut own = RuntimeLayout::default();
    own.add(module);
    let location = |owner: &str, symbol: &str| {
        own.location(owner, symbol)
            .or_else(|| layout.location(owner, symbol))
    };
    let mut parts: BTreeMap<Option<u32>, Vec<Item>> = BTreeMap::new();
    parts.insert(None, Vec::new());
    for (item, phase) in module.items.iter().zip(phases(module)) {
        parts.entry(phase).or_default().push(item.clone());
    }
    let prefixes: Vec<_> = parts
        .keys()
        .flatten()
        .map(|id| prefix_key(&module.name, *id))
        .collect();
    parts
        .into_iter()
        .map(|(phase, items)| {
            let physical =
                phase.map_or_else(|| module.name.clone(), |id| prefix_key(&module.name, id));
            let part = Module {
                name: module.name.clone(),
                items,
                labels: module.labels.clone(),
                initialization: module.initialization.clone(),
            };
            let mut output = super::js::js_at(&part, options, &physical, &location);
            if phase.is_none() {
                let here = (options.path)(&module.name);
                for prefix in &prefixes {
                    let _ = writeln!(
                        output.code,
                        "export * from {};",
                        names::string(&super::relative(&here, &(options.path)(prefix)))
                    );
                }
                output.imports.extend(prefixes.clone());
            }
            (physical, output)
        })
        .collect()
}
