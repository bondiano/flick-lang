//! Reject reachable bindings that RT cannot run without the optional adapter.
//! A module may contain unavailable bindings; an app only needs what its entry
//! and eager initializers reach, while a library promises its public exports.

use std::collections::HashSet;

use super::{Settings, builtin_modules_of, effects};
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::Namespace;
use crate::ir::{Expr, ExprKind, Foreign, ForeignMode, Item, ItemKind, Module, reach};
use crate::project::Compilation;
use crate::source::Span;
use crate::types::builtins_of;

pub(super) fn check(
    compilation: &Compilation,
    settings: &Settings<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let modules: Vec<_> = compilation
        .modules
        .iter()
        .map(|module| &module.ir)
        .chain(builtin_modules_of(true))
        .collect();
    let index = reach::index(&modules);
    let mut pending = roots(compilation, settings, &modules);
    let mut seen = HashSet::new();
    while let Some((key, origin)) = pending.pop() {
        let Some(&at) = index.get(&key) else {
            // An external module's implementation is a foreign trust boundary.
            continue;
        };
        if !seen.insert(at) {
            continue;
        }
        let item = &modules[at.0].items[at.1];
        if let ItemKind::Foreign(foreign) = &item.kind
            && needs_adapter(foreign)
        {
            diagnostics.push(Diagnostic::error(
                codes::P012,
                format!(
                    "`{}.{}` requires the optional `{}` adapter; \
                     add it to the project's dependencies to build this program on Flick's runtime",
                    key.0,
                    key.1,
                    super::bindings::ADAPTER_PREFIX
                ),
                origin,
            ));
            continue;
        }
        let mut used = Vec::new();
        reach::uses(item, &mut used);
        for key in used {
            let span = if at.0 < compilation.modules.len() {
                reference_span(item, &key).unwrap_or(origin)
            } else {
                origin
            };
            pending.push((key, span));
        }
    }
}

fn roots(
    compilation: &Compilation,
    settings: &Settings<'_>,
    modules: &[&Module],
) -> Vec<(reach::Key, Span)> {
    let mut pending = Vec::new();
    let mut entries: HashSet<_> = settings
        .entries
        .iter()
        .filter_map(|(_, entry)| {
            compilation
                .modules
                .iter()
                .find(|module| &module.name == entry)?
                .interface
                .find(Namespace::Value, "main")
                .map(|export| (export.entity.module.clone(), export.entity.name.clone()))
        })
        .collect();
    // A custom runner is part of the executable even when main never refers
    // to it. Resolve re-exports just as for main.
    for (_, runner) in settings.runners {
        let interface = compilation
            .modules
            .iter()
            .find(|module| &module.name == runner)
            .map(|module| &module.interface)
            .or_else(|| {
                builtins_of(true)
                    .iter()
                    .find(|module| &module.module == runner)
            });
        if let Some(export) = interface.and_then(|module| module.find(Namespace::Value, "run")) {
            let key = (export.entity.module.clone(), export.entity.name.clone());
            if compilation
                .modules
                .iter()
                .any(|module| module.name == key.0)
            {
                entries.insert(key);
            } else if let Some(origin) = compilation
                .modules
                .iter()
                .flat_map(|module| &module.ir.items)
                .next()
            {
                pending.push((key, origin.span));
            }
        }
    }
    let mut loading: Vec<_> = entries.iter().map(|(module, _)| module.clone()).collect();
    loading.extend(settings.runners.iter().map(|(_, module)| module.clone()));
    if settings.entries.is_empty() {
        loading.extend(
            compilation
                .modules
                .iter()
                .filter(|module| module.package.is_none() && !module.interface.private)
                .map(|module| module.name.clone()),
        );
    }
    let loaded = loaded_modules(modules, loading);
    for module in &compilation.modules {
        for item in &module.ir.items {
            let eager = loaded.contains(&module.name)
                && match &item.kind {
                    ItemKind::Value { dicts, body, .. } => {
                        dicts.is_empty()
                            && !matches!(
                                body.kind,
                                ExprKind::Lambda { .. }
                                    | ExprKind::Task(_)
                                    | ExprKind::Global { .. }
                                    | ExprKind::Abstract(..)
                            )
                    }
                    ItemKind::Destructure { .. } => true,
                    _ => false,
                };
            for name in reach::defined(item) {
                let entry = entries.contains(&(module.name.clone(), name.clone()));
                let public = settings.entries.is_empty()
                    && module.package.is_none()
                    && !module.interface.private
                    && (module.interface.exports_value(&name)
                        || matches!(item.kind, ItemKind::Instance { .. }));
                if entry || public || eager {
                    pending.push(((module.name.clone(), name), item.span));
                }
            }
        }
    }
    pending
}

/// Static imports load a module even when the referencing function is unused.
fn loaded_modules(modules: &[&Module], mut pending: Vec<String>) -> HashSet<String> {
    let mut loaded = HashSet::new();
    while let Some(name) = pending.pop() {
        if !loaded.insert(name.clone()) {
            continue;
        }
        if let Some(module) = modules.iter().find(|module| module.name == name) {
            let mut used = Vec::new();
            for item in &module.items {
                reach::uses(item, &mut used);
            }
            pending.extend(used.into_iter().map(|(module, _)| module));
        }
    }
    loaded
}

fn needs_adapter(foreign: &Foreign) -> bool {
    let Some(from) = foreign.binding.from.as_deref() else {
        return false;
    };
    let native = effects::rt_specifier(from);
    effects::requires_adapter(
        native.as_deref().unwrap_or(from),
        &foreign.binding.name,
        foreign.mode == ForeignMode::Effect,
    )
}

fn reference_span(item: &Item, key: &reach::Key) -> Option<Span> {
    fn find(expr: &Expr, key: &reach::Key, found: &mut Option<Span>) {
        if let ExprKind::Global { module, name } = &expr.kind
            && (module, name) == (&key.0, &key.1)
        {
            found.get_or_insert(expr.span);
        }
        expr.each_child(&mut |child| find(child, key, found));
    }
    let mut found = None;
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
            find(body, key, &mut found);
        }
        ItemKind::Instance { methods, .. } => {
            for (_, method) in methods {
                find(method, key, &mut found);
            }
        }
        ItemKind::Refined { check, .. } => find(check, key, &mut found),
        ItemKind::Foreign(foreign) => {
            if let Some(catch) = &foreign.catch {
                find(catch, key, &mut found);
            }
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
    }
    found
}
