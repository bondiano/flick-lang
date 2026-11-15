//! Environment-specific effect preflight: reachable gated services need a
//! root handler for the selected backend/platform pair.

use std::collections::HashSet;

use super::{Settings, builtin_modules_of};
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::Namespace;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Module, reach};
use crate::project::Compilation;
use crate::project::config::BackendEnvironment;
use crate::resolve::{self, ItemKind as ExportKind};
use crate::source::{SourceMap, Span};
use crate::syntax;
use crate::types::builtins_of;

/// Services that need an explicit environment root handler.
const GATED: &[&str] = &["Database", "Db", "Server"];

/// Whether `effect` (`Module.Service`) is gated and, if so, its short name.
pub fn gated_service(effect: &str) -> Option<&str> {
    let name = effect.rsplit('.').next()?;
    GATED.iter().copied().find(|&gated| gated == name)
}

/// Short names of gated services that have a root handler on `environment`.
pub fn handled_services(environment: &BackendEnvironment) -> &'static [&'static str] {
    match (environment.backend.as_str(), environment.platform.as_str()) {
        ("js", "node") | ("go", "native") => GATED,
        _ => &[],
    }
}

/// Whether `environment` has a root handler for the gated service `name`.
pub fn handles(environment: &BackendEnvironment, name: &str) -> bool {
    handled_services(environment).contains(&name)
}

/// Std root-handler symbols declared for an environment that hosts them.
/// `Db` is gated in source checks but has no fixed std handler symbol.
pub fn std_root_handlers(
    environment: &BackendEnvironment,
) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    if !matches!(
        (environment.backend.as_str(), environment.platform.as_str()),
        ("js", "node") | ("go", "native")
    ) {
        return Vec::new();
    }
    vec![
        ("Database", "Database", "Database", "pg"),
        ("Server", "Server", "Server", "handler"),
    ]
}

/// Reject reachable gated effects that the selected environment cannot host.
pub fn check(
    compilation: &Compilation,
    settings: &Settings<'_>,
    sources: &SourceMap,
    environment: &BackendEnvironment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let modules: Vec<_> = compilation
        .modules
        .iter()
        .map(|module| &module.ir)
        .chain(builtin_modules_of(true))
        .collect();
    let index = reach::index(&modules);
    let mut pending = roots(compilation, settings, &modules, sources);
    let mut seen = HashSet::new();
    let mut reported = HashSet::new();
    while let Some((key, origin)) = pending.pop() {
        let Some(&at) = index.get(&key) else {
            continue;
        };
        if !seen.insert(at) {
            continue;
        }
        let item = &modules[at.0].items[at.1];
        if at.0 < compilation.modules.len() {
            scan_item(item, environment, &mut reported, diagnostics);
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

fn scan_item(
    item: &Item,
    environment: &BackendEnvironment,
    reported: &mut HashSet<(String, Span)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    item.each_body(&mut |expr| {
        scan_expr(expr, environment, &HashSet::new(), reported, diagnostics);
    });
}

fn scan_expr(
    expr: &Expr,
    environment: &BackendEnvironment,
    covered: &HashSet<String>,
    reported: &mut HashSet<(String, Span)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &expr.kind {
        ExprKind::Perform { effect, .. } => {
            if let Some(name) = gated_service(effect)
                && !covered.contains(effect)
                && !covered.contains(name)
                && !handles(environment, name)
            {
                let key = (name.to_owned(), expr.span);
                if reported.insert(key) {
                    diagnostics.push(Diagnostic::error(
                        codes::E020,
                        format!(
                            "unhandled effect {name} on {}/{}",
                            environment.backend, environment.platform
                        ),
                        expr.span,
                    ));
                }
            }
            expr.each_child(&mut |child| {
                scan_expr(child, environment, covered, reported, diagnostics);
            });
        }
        ExprKind::Handle(handler) => {
            let mut nested = covered.clone();
            for handled in &handler.effects {
                nested.insert(handled.effect.clone());
                if let Some(name) = gated_service(&handled.effect) {
                    nested.insert(name.to_owned());
                }
            }
            // Operation implementations are checked in the outer coverage;
            // only the body gains the handler's effects.
            for handled in &handler.effects {
                for arm in &handled.operations {
                    scan_expr(&arm.function, environment, covered, reported, diagnostics);
                }
            }
            scan_expr(&handler.body, environment, &nested, reported, diagnostics);
            if let Some(ret) = &handler.ret {
                scan_expr(ret, environment, covered, reported, diagnostics);
            }
            for arm in &handler.errors {
                if let Some(guard) = &arm.guard {
                    scan_expr(guard, environment, covered, reported, diagnostics);
                }
                scan_expr(&arm.body, environment, covered, reported, diagnostics);
            }
        }
        _ => {
            expr.each_child(&mut |child| {
                scan_expr(child, environment, covered, reported, diagnostics);
            });
        }
    }
}

fn roots(
    compilation: &Compilation,
    settings: &Settings<'_>,
    modules: &[&Module],
    sources: &SourceMap,
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
    loading.extend(settings.entries.iter().map(|(_, module)| module.clone()));
    loading.extend(settings.runners.iter().map(|(_, module)| module.clone()));
    if settings.entries.is_empty() {
        loading.extend(
            compilation
                .modules
                .iter()
                .filter(|module| module.package.is_none() && !module.interface.private)
                .map(|module| module.name.clone()),
        );
        let reexports = library_reexports(compilation, sources);
        loading.extend(reexports.iter().map(|(key, _)| key.0.clone()));
        pending.extend(reexports);
    }
    let loaded = loaded_modules(compilation, modules, loading);
    for module in &compilation.modules {
        for item in &module.ir.items {
            if loaded.contains(&module.name) {
                initialization_roots(item, &mut pending);
            }
            for name in reach::defined(item) {
                let entry = entries.contains(&(module.name.clone(), name.clone()));
                let public = settings.entries.is_empty()
                    && module.package.is_none()
                    && !module.interface.private
                    && (module.interface.exports_value(&name)
                        || matches!(item.kind, ItemKind::Instance { .. }));
                if entry || public {
                    pending.push(((module.name.clone(), name), item.span));
                }
            }
        }
    }
    pending
}

fn initialization_roots(item: &Item, pending: &mut Vec<(reach::Key, Span)>) {
    fn visit(expr: &Expr, pending: &mut Vec<(reach::Key, Span)>) {
        if matches!(
            expr.kind,
            ExprKind::Lambda { .. } | ExprKind::Task(_) | ExprKind::Abstract(..)
        ) {
            return;
        }
        let mut used = Vec::new();
        match &expr.kind {
            ExprKind::Global { module, name } => used.push((module.clone(), name.clone())),
            ExprKind::Method { dict, .. } => reach::dict_uses(dict, &mut used),
            ExprKind::Dicts(_, dicts) => {
                for dict in dicts {
                    reach::dict_uses(dict, &mut used);
                }
            }
            _ => {}
        }
        pending.extend(used.into_iter().map(|key| (key, expr.span)));
        expr.each_child(&mut |child| visit(child, pending));
    }
    match &item.kind {
        ItemKind::Value { dicts, body, .. }
            if dicts.is_empty() && !matches!(body.kind, ExprKind::Global { .. }) =>
        {
            visit(body, pending);
        }
        ItemKind::Destructure { body, .. } => visit(body, pending),
        ItemKind::Instance { info, methods } if info.context.is_empty() => {
            for (_, body) in methods {
                if !matches!(body.kind, ExprKind::Global { .. }) {
                    visit(body, pending);
                }
            }
        }
        _ => {}
    }
}

fn library_reexports(compilation: &Compilation, sources: &SourceMap) -> Vec<(reach::Key, Span)> {
    let mut pending = Vec::new();
    for module in &compilation.modules {
        if module.package.is_some() || module.interface.private {
            continue;
        }
        let reexports: Vec<_> = module
            .interface
            .exports
            .iter()
            .filter(|export| {
                export.entity.namespace == Namespace::Value && export.entity.module != module.name
            })
            .collect();
        if reexports.is_empty() {
            continue;
        }
        let text = &sources.get(module.file).text;
        let parsed = syntax::parse(text, module.file);
        let syntax = resolve::module_syntax(&parsed.tree, text, module.file);
        for export in reexports {
            let Some(header) = &syntax.header else {
                continue;
            };
            let span = header
                .exposing
                .as_ref()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item.name == export.entity.name)
                        .or_else(|| items.iter().find(|item| item.kind == ExportKind::All))
                })
                .map_or(header.span, |item| item.span);
            let key = (export.entity.module.clone(), export.entity.name.clone());
            pending.push((key, span));
        }
    }
    pending
}

fn loaded_modules(
    compilation: &Compilation,
    modules: &[&Module],
    mut pending: Vec<String>,
) -> HashSet<String> {
    let mut loaded = HashSet::new();
    while let Some(name) = pending.pop() {
        if !loaded.insert(name.clone()) {
            continue;
        }
        if let Some(module) = compilation
            .modules
            .iter()
            .find(|module| module.name == name)
        {
            pending.extend(
                module
                    .interface
                    .exports
                    .iter()
                    .filter(|export| {
                        export.entity.namespace == Namespace::Value && export.entity.module != name
                    })
                    .map(|export| export.entity.module.clone()),
            );
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
    item.each_body(&mut |body| find(body, key, &mut found));
    found
}
