//! Resolve checked builtin fallbacks at the caller, not in the generic lowerer.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use flick_core::build;
use flick_core::iface::Interface;
use flick_core::ir::{Item, ItemKind, Module, Pat, memory::Name};
use flick_core::project::Compiled;

pub(super) fn declarations(modules: &[Compiled]) -> Result<Vec<Cow<'_, Interface>>, String> {
    let mut roots = BTreeMap::new();
    let mut resolved = Vec::new();
    for module in modules {
        let declaration = module.declarations.as_ref();
        if roots
            .insert(declaration.module.as_str(), resolved.len())
            .is_some()
        {
            return Err(format!(
                "invalid counted declaration context: duplicate declaration context {}",
                declaration.module
            ));
        }
        resolved.push(Cow::Borrowed(declaration));
    }
    for builtin in build::builtin_declarations() {
        if let Some(&index) = roots.get(builtin.module.as_str()) {
            fill_builtin(resolved[index].to_mut(), builtin);
        } else {
            resolved.push(Cow::Borrowed(builtin.as_ref()));
        }
    }
    Ok(resolved)
}

fn fill_builtin(explicit: &mut Interface, builtin: &Interface) {
    // An explicit private/open definition is just as authoritative as an
    // explicit export. A builtin public projection must not replace it.
    let mut entities: BTreeSet<_> = explicit
        .exports
        .iter()
        .map(|export| export.entity.clone())
        .chain(
            explicit
                .support_types
                .iter()
                .map(|support| support.entity.clone()),
        )
        .collect();
    for export in &builtin.exports {
        if entities.insert(export.entity.clone()) {
            explicit.exports.push(export.clone());
        }
    }
    for support in &builtin.support_types {
        if entities.insert(support.entity.clone()) {
            // Insert the fallback without reordering any original facts. An
            // invalid explicit order must still fail the generic validator.
            let index = explicit
                .support_types
                .iter()
                .position(|previous| previous.entity > support.entity)
                .unwrap_or(explicit.support_types.len());
            explicit.support_types.insert(index, support.clone());
        }
    }
    for (name, child) in &builtin.modules {
        // Keep a checked explicit child view intact, including its opacity.
        explicit
            .modules
            .entry(name.clone())
            .or_insert_with(|| child.clone());
    }
    explicit.incomplete |= builtin.incomplete;
}

fn globals(module: &Module, item: &Item) -> Vec<Name> {
    match &item.kind {
        ItemKind::Value { name, .. } => vec![Name::new(&module.name, name)],
        ItemKind::Foreign(foreign) => vec![Name::new(&module.name, &foreign.name)],
        ItemKind::Destructure { pattern, .. } => {
            let mut names = Vec::new();
            pattern.binders(&mut names);
            names
                .into_iter()
                .map(|name| Name::new(&module.name, name))
                .collect()
        }
        ItemKind::Instance { info, .. } => {
            vec![Name::new(&info.module, &format!("instance {}", info.name))]
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => Vec::new(),
    }
}

pub(super) fn modules(explicit: &[Module]) -> Vec<Cow<'_, Module>> {
    let shadowed: BTreeSet<_> = explicit
        .iter()
        .flat_map(|module| module.items.iter().flat_map(|item| globals(module, item)))
        .collect();
    let mut resolved: Vec<_> = explicit.iter().map(Cow::Borrowed).collect();
    for builtin in build::builtin_modules() {
        resolved.push(fallback_module(builtin, &shadowed));
    }
    resolved
}

fn fallback_module<'a>(builtin: &'a Module, shadowed: &BTreeSet<Name>) -> Cow<'a, Module> {
    if !builtin.items.iter().any(|item| {
        globals(builtin, item)
            .iter()
            .any(|name| shadowed.contains(name))
    }) {
        return Cow::Borrowed(builtin);
    }
    let mut fallback = builtin.clone();
    fallback.items.retain_mut(|item| {
        let names = globals(builtin, item);
        if names.is_empty() || names.iter().all(|name| !shadowed.contains(name)) {
            return true;
        }
        if names.iter().all(|name| shadowed.contains(name)) {
            return false;
        }
        // A multi-binder initializer still supplies its unshadowed values.
        // Preserve all matching constraints and the initializer itself.
        if let ItemKind::Destructure { pattern, .. } = &mut item.kind {
            omit_shadowed(pattern, &builtin.name, shadowed);
        }
        true
    });
    Cow::Owned(fallback)
}

fn omit_shadowed(pattern: &mut Pat, module: &str, shadowed: &BTreeSet<Name>) {
    match pattern {
        Pat::Bind(name) if shadowed.contains(&Name::new(module, name)) => *pattern = Pat::Wild,
        Pat::Bind(_) | Pat::Wild | Pat::Lit(_) => {}
        Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
            for item in items {
                omit_shadowed(item, module, shadowed);
            }
        }
        Pat::List(items, rest) => {
            for item in items.iter_mut().chain(rest.as_deref_mut()) {
                omit_shadowed(item, module, shadowed);
            }
        }
        Pat::Record(fields) => {
            for (_, item) in fields {
                omit_shadowed(item, module, shadowed);
            }
        }
    }
}

#[cfg(test)]
mod tests;
