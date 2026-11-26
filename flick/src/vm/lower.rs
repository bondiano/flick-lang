//! Counted memory-IR lowering for in-process VM execution.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use flick_core::build;
use flick_core::iface::Interface;
use flick_core::ir::memory::{self, Name as MemoryName};
use flick_core::ir::{Item, ItemKind, Module, Pat};
use flick_core::opt::perceus;
use flick_core::project::{Compilation, Compiled};
use flick_core::types::NativeOverrideEnv;

/// A lowered memory-IR program and its optional `Host.live` root layer.
pub(super) struct LoweredProgram {
    pub(super) program: memory::Program,
    pub(super) root: Option<MemoryName>,
}

/// Lowers `entries` (plus `Host.live` when present) from `compilation` to `flick_ir::memory::Program`.
pub(super) fn lower(
    compilation: &Compilation,
    entries: &[MemoryName],
    release: bool,
) -> Result<LoweredProgram, String> {
    let modules = &compilation.modules;
    let explicit_ir: Vec<Module> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(modules, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    let declarations = declarations(modules)?;
    let ir = combined_modules(&explicit_ir, compilation.native_overrides.as_ref());
    let root_layer = MemoryName::new("Host", "live");
    let has_root = ir.iter().any(|m| {
        m.name == root_layer.module
            && m.items.iter().any(
                |i| matches!(&i.kind, ItemKind::Value { name, .. } if *name == root_layer.name),
            )
    });
    let root = has_root.then_some(root_layer);
    let roots: Vec<&MemoryName> = entries.iter().chain(&root).collect();
    let program = perceus::lower_roots_with_declarations(
        ir.iter().map(AsRef::as_ref),
        declarations.iter().map(AsRef::as_ref),
        &roots,
    )
    .map_err(|error| error.to_string())?;
    Ok(LoweredProgram { program, root })
}

fn declarations(modules: &[Compiled]) -> Result<Vec<Cow<'_, Interface>>, String> {
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
            let index = explicit
                .support_types
                .iter()
                .position(|previous| previous.entity > support.entity)
                .unwrap_or(explicit.support_types.len());
            explicit.support_types.insert(index, support.clone());
        }
    }
    for (name, child) in &builtin.modules {
        explicit
            .modules
            .entry(name.clone())
            .or_insert_with(|| child.clone());
    }
    explicit.incomplete |= builtin.incomplete;
}

fn globals(module: &Module, item: &Item) -> Vec<MemoryName> {
    match &item.kind {
        ItemKind::Value { name, .. } => vec![MemoryName::new(&module.name, name)],
        ItemKind::Foreign(foreign) => vec![MemoryName::new(&module.name, &foreign.name)],
        ItemKind::Destructure { pattern, .. } => {
            let mut names = Vec::new();
            pattern.binders(&mut names);
            names
                .into_iter()
                .map(|name| MemoryName::new(&module.name, name))
                .collect()
        }
        ItemKind::Instance { info, .. } => {
            vec![MemoryName::new(
                &info.module,
                &format!("instance {}", info.name),
            )]
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => Vec::new(),
    }
}

fn combined_modules<'a>(
    explicit: &'a [Module],
    env: Option<&NativeOverrideEnv>,
) -> Vec<Cow<'a, Module>> {
    let shadowed: BTreeSet<_> = explicit
        .iter()
        .flat_map(|module| module.items.iter().flat_map(|item| globals(module, item)))
        .collect();
    let mut resolved: Vec<Cow<'a, Module>> = explicit.iter().map(Cow::Borrowed).collect();
    let bundle = build::builtin_bundle_of(false);
    for (i, builtin) in bundle.modules.iter().enumerate() {
        let selected = if env.is_some() && !bundle.alternatives.get(i).is_none_or(Vec::is_empty) {
            Cow::Owned(bundle.module_for(i, env))
        } else {
            Cow::Borrowed(builtin)
        };
        resolved.push(fallback_module(selected, &shadowed));
    }
    resolved
}

fn fallback_module<'a>(
    builtin: Cow<'a, Module>,
    shadowed: &BTreeSet<MemoryName>,
) -> Cow<'a, Module> {
    if !builtin.items.iter().any(|item| {
        globals(&builtin, item)
            .iter()
            .any(|name| shadowed.contains(name))
    }) {
        return builtin;
    }
    let mut fallback = builtin.into_owned();
    let module_name = fallback.name.clone();
    fallback.items.retain_mut(|item| {
        let names = item_globals(&module_name, item);
        if names.is_empty() || names.iter().all(|name| !shadowed.contains(name)) {
            return true;
        }
        if names.iter().all(|name| shadowed.contains(name)) {
            return false;
        }
        if let ItemKind::Destructure { pattern, .. } = &mut item.kind {
            omit_shadowed(pattern, &module_name, shadowed);
        }
        true
    });
    Cow::Owned(fallback)
}

fn item_globals(module_name: &str, item: &Item) -> Vec<MemoryName> {
    match &item.kind {
        ItemKind::Value { name, .. } => vec![MemoryName::new(module_name, name)],
        ItemKind::Foreign(foreign) => vec![MemoryName::new(module_name, &foreign.name)],
        ItemKind::Destructure { pattern, .. } => {
            let mut names = Vec::new();
            pattern.binders(&mut names);
            names
                .into_iter()
                .map(|name| MemoryName::new(module_name, name))
                .collect()
        }
        ItemKind::Instance { info, .. } => {
            vec![MemoryName::new(
                &info.module,
                &format!("instance {}", info.name),
            )]
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => Vec::new(),
    }
}

fn omit_shadowed(pattern: &mut Pat, module: &str, shadowed: &BTreeSet<MemoryName>) {
    match pattern {
        Pat::Bind(name) if shadowed.contains(&MemoryName::new(module, name)) => {
            *pattern = Pat::Wild;
        }
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
