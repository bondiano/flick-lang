//! Environment-aware pruning for builds and test runs.
//!
//! After preflight proves no reachable root depends on an unavailable module,
//! facade, or missing Effect-adapter binding, this pass drops gated-out modules
//! and backward-taints dead items in surviving modules that reference them so
//! backends never emit unavailable modules or top-level static imports to them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use flick_ir::Namespace as IrNamespace;

use super::facades::{FacadeUse, item_facade_uses, module_facade_uses};
use super::gated::needs_adapter;
use super::locals::{deferred_local, reachable_block_stmts};
use super::refs::{Reference, collect_expr_references, collect_item_references};
use crate::iface::Namespace;
use crate::ir::{Expr, ExprKind, Item, ItemKind, reach};
use crate::project::builtins::builtin_modules_of;
use crate::project::environment::ResolvedEnvironment;
use crate::project::{Compilation, Compiled};
use crate::source::SourceMap;
use crate::types::builtins_of;

/// Returns a pruned copy of `compilation` for its resolved environment (or a
/// straight clone when no environment was resolved).
pub(crate) fn prune_compilation(
    compilation: &Compilation,
    sources: &SourceMap,
    by_class: bool,
) -> Compilation {
    let Some(env) = &compilation.environment else {
        return compilation.clone();
    };
    prune_compilation_for_env(compilation, sources, by_class, env)
}

/// Returns a pruned copy of `compilation` for an explicit `ResolvedEnvironment`.
pub(crate) fn prune_compilation_for_env(
    compilation: &Compilation,
    sources: &SourceMap,
    by_class: bool,
    env: &ResolvedEnvironment,
) -> Compilation {
    let unavailable = collect_unavailable_modules(compilation, env, by_class);
    let check_adapter = env.tags.contains("js")
        && !env.tags.contains("effect")
        && !env.tags.contains("effect-adapter");

    // Collect facade accesses for surviving user modules that might reference an
    // unavailable re-exporting facade.
    let facade_uses: HashMap<String, Vec<FacadeUse>> = if unavailable.is_empty() {
        HashMap::new()
    } else {
        compilation
            .modules
            .iter()
            .filter(|m| !unavailable.contains(&m.name))
            .map(|m| {
                (
                    m.name.clone(),
                    module_facade_uses(m, compilation, sources, by_class),
                )
            })
            .collect()
    };

    let (tainted_keys, tainted_items) = compute_tainted(
        compilation,
        env,
        by_class,
        check_adapter,
        &unavailable,
        &facade_uses,
    );

    let mut out = compilation.clone();
    out.modules.retain(|m| !unavailable.contains(&m.name));
    for module in &mut out.modules {
        prune_module(module, &unavailable, &tainted_keys, &tainted_items);
    }
    out
}

fn collect_unavailable_modules(
    compilation: &Compilation,
    env: &ResolvedEnvironment,
    by_class: bool,
) -> HashSet<String> {
    let mut unavailable = HashSet::new();
    for builtin in builtins_of(by_class) {
        if let Some(cond) = &builtin.when
            && !env.matches(cond)
        {
            unavailable.insert(builtin.module.clone());
        }
    }
    for compiled in &compilation.modules {
        let own_ok = compiled
            .interface
            .when
            .as_deref()
            .is_none_or(|cond| env.matches(cond));
        let pkg_cond = match &compiled.package {
            Some(pkg) => compilation.package_when.get(pkg).map(String::as_str),
            None => compilation
                .own_package_when
                .as_ref()
                .map(|(_, c)| c.as_str()),
        };
        let pkg_ok = pkg_cond.is_none_or(|cond| env.matches(cond));
        if !own_ok || !pkg_ok {
            unavailable.insert(compiled.name.clone());
        }
    }
    unavailable
}

/// Computes the fixed-point set of tainted `(module, name)` keys (across both
/// user and built-in modules) and tainted `(module_name, item_idx)` pairs in
/// surviving user modules.
fn compute_tainted(
    compilation: &Compilation,
    env: &ResolvedEnvironment,
    by_class: bool,
    check_adapter: bool,
    unavailable: &HashSet<String>,
    facade_uses: &HashMap<String, Vec<FacadeUse>>,
) -> (HashSet<reach::Key>, HashSet<(String, usize)>) {
    let mut tainted_keys: HashSet<reach::Key> = HashSet::new();
    let mut tainted_items: HashSet<(String, usize)> = HashSet::new();
    let builtins = builtin_modules_of(by_class);

    // Seed adapter-only foreign items when running on JS `rt` without the adapter.
    if check_adapter {
        let seed_module = |module: &flick_ir::Module,
                           record_index: bool,
                           tainted_keys: &mut HashSet<reach::Key>,
                           tainted_items: &mut HashSet<(String, usize)>| {
            for (idx, item) in module.items.iter().enumerate() {
                if let ItemKind::Foreign(foreign) = &item.kind
                    && needs_adapter(foreign, env.ui.as_deref())
                {
                    if record_index {
                        tainted_items.insert((module.name.clone(), idx));
                    }
                    for name in reach::defined(item) {
                        tainted_keys.insert((module.name.clone(), name));
                    }
                }
            }
        };
        for compiled in &compilation.modules {
            if !unavailable.contains(&compiled.name) {
                seed_module(&compiled.ir, true, &mut tainted_keys, &mut tainted_items);
            }
        }
        for builtin in builtins {
            if !unavailable.contains(&builtin.name) {
                seed_module(builtin, false, &mut tainted_keys, &mut tainted_items);
            }
        }
    }

    // Fixed-point backward taint propagation: any item referencing an unavailable
    // module/facade or a tainted item becomes tainted itself.
    let mut changed = true;
    while changed {
        changed = false;
        for compiled in &compilation.modules {
            if unavailable.contains(&compiled.name) {
                continue;
            }
            let mod_facades = facade_uses
                .get(&compiled.name)
                .map_or(&[][..], Vec::as_slice);
            for (idx, item) in compiled.ir.items.iter().enumerate() {
                if tainted_items.contains(&(compiled.name.clone(), idx)) {
                    continue;
                }
                if item_is_tainted(item, true, mod_facades, unavailable, &tainted_keys) {
                    tainted_items.insert((compiled.name.clone(), idx));
                    for name in reach::defined(item) {
                        tainted_keys.insert((compiled.name.clone(), name));
                    }
                    changed = true;
                }
            }
        }
        for builtin in builtins {
            if unavailable.contains(&builtin.name) {
                continue;
            }
            for item in &builtin.items {
                let names = reach::defined(item);
                if names
                    .iter()
                    .all(|n| tainted_keys.contains(&(builtin.name.clone(), n.clone())))
                {
                    continue;
                }
                if item_is_tainted(item, false, &[], unavailable, &tainted_keys) {
                    for name in names {
                        if tainted_keys.insert((builtin.name.clone(), name)) {
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    (tainted_keys, tainted_items)
}

fn item_is_tainted(
    item: &flick_ir::Item,
    is_user_module: bool,
    mod_facades: &[FacadeUse],
    unavailable: &HashSet<String>,
    tainted_keys: &HashSet<reach::Key>,
) -> bool {
    if is_user_module
        && item_facade_uses(mod_facades, item)
            .into_iter()
            .any(|u| u.facades.iter().any(|f| unavailable.contains(f)))
    {
        return true;
    }
    let mut refs = Vec::new();
    collect_item_references(item, is_user_module, None, 0, &mut refs);
    refs.into_iter().any(|r| match r {
        Reference::Value { key, .. } => unavailable.contains(&key.0) || tainted_keys.contains(&key),
        Reference::ModuleOnly { module, .. } => unavailable.contains(&module),
    })
}

fn strip_tainted_dead_locals(
    item: &mut Item,
    unavailable: &HashSet<String>,
    tainted_keys: &HashSet<reach::Key>,
) {
    fn walk(expr: &mut Expr, unavailable: &HashSet<String>, tainted_keys: &HashSet<reach::Key>) {
        if let ExprKind::Block(stmts, result) = &mut expr.kind {
            let active = reachable_block_stmts(stmts, result);
            let mut keep = Vec::with_capacity(stmts.len());
            for (stmt, is_active) in stmts.iter().zip(active) {
                if !is_active && let Some((_, _, value)) = deferred_local(stmt) {
                    let mut refs = Vec::new();
                    collect_expr_references(value, &|s| Some(s), 0, &mut refs);
                    let tainted = refs.into_iter().any(|r| match r {
                        Reference::Value { key, .. } => {
                            unavailable.contains(&key.0) || tainted_keys.contains(&key)
                        }
                        Reference::ModuleOnly { module, .. } => unavailable.contains(&module),
                    });
                    if tainted {
                        keep.push(false);
                        continue;
                    }
                }
                keep.push(true);
            }
            let mut idx = 0usize;
            stmts.retain(|_| {
                let k = keep[idx];
                idx += 1;
                k
            });
        }
        expr.each_child_mut(&mut |child| walk(child, unavailable, tainted_keys));
    }

    match &mut item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
            walk(body, unavailable, tainted_keys);
        }
        ItemKind::Instance { methods, .. } => {
            for (_, body) in methods {
                walk(body, unavailable, tainted_keys);
            }
        }
        ItemKind::Foreign(foreign) => {
            if let Some(catch) = &mut foreign.catch {
                walk(catch, unavailable, tainted_keys);
            }
        }
        ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
    }
}

fn prune_module(
    module: &mut Compiled,
    unavailable: &HashSet<String>,
    tainted_keys: &HashSet<reach::Key>,
    tainted_items: &HashSet<(String, usize)>,
) {
    let mod_name = module.name.clone();
    let ir = Arc::make_mut(&mut module.ir);
    let mut idx = 0usize;
    ir.items.retain(|_| {
        let keep = !tainted_items.contains(&(mod_name.clone(), idx));
        idx += 1;
        keep
    });
    if !unavailable.is_empty() || !tainted_keys.is_empty() {
        for item in &mut ir.items {
            strip_tainted_dead_locals(item, unavailable, tainted_keys);
        }
    }
    module.imports.retain(|imp| !unavailable.contains(imp));
    ir.labels
        .retain(|name, _| !tainted_keys.contains(&(mod_name.clone(), name.clone())));
    for prefix in &mut ir.initialization.prefixes {
        prefix
            .bindings
            .retain(|name| !tainted_keys.contains(&(mod_name.clone(), name.clone())));
    }
    ir.initialization
        .prefixes
        .retain(|p| !p.bindings.is_empty());

    // Drop exports & re-exports pointing at unavailable modules, unavailable
    // facades, or tainted items.
    module.interface.exports.retain(|export| {
        if unavailable.contains(&export.entity.module)
            || export.via.iter().any(|v| unavailable.contains(v))
        {
            return false;
        }
        if export.entity.namespace == Namespace::Value
            && tainted_keys.contains(&(export.entity.module.clone(), export.entity.name.clone()))
        {
            return false;
        }
        true
    });
    ir.exports
        .values
        .retain(|e| !tainted_keys.contains(&(mod_name.clone(), e.name.clone())));
    ir.exports.reexports.retain(|re| {
        if unavailable.contains(&re.module) {
            return false;
        }
        if re.namespace == IrNamespace::Value
            && tainted_keys.contains(&(re.module.clone(), re.name.clone()))
        {
            return false;
        }
        module.interface.exports.iter().any(|e| {
            e.entity.module == re.module
                && e.entity.name == re.name
                && matches!(
                    (e.entity.namespace, re.namespace),
                    (Namespace::Value, IrNamespace::Value)
                        | (Namespace::Type, IrNamespace::Type)
                        | (Namespace::Constructor, IrNamespace::Constructor)
                )
        })
    });
}
