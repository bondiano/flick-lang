//! Closed module layout, ownership and root dependencies of a request.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::Program;

use crate::validate::{Limits, path};
use crate::{BuildRequest, InputKind, ModuleInput, ModuleRole};

pub(super) fn modules<'r>(
    request: &'r BuildRequest,
    program: &Program,
    manifest: &BTreeMap<&str, InputKind>,
    limits: Limits,
) -> Result<BTreeMap<&'r str, &'r ModuleInput>, String> {
    let mut modules = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for module in &request.modules {
        if modules.insert(module.name.as_str(), module).is_some() {
            return Err("duplicate module metadata".into());
        }
        path::insert(&module.output, &mut paths, limits)?;
        if let Some(interface) = &module.interface
            && manifest.get(interface.as_str()) != Some(&InputKind::Interface)
        {
            return Err("module interface is not a declared interface input".into());
        }
        if module.role == ModuleRole::External && module.interface.is_none() {
            return Err("external module has no explicit interface ABI".into());
        }
    }
    for module in &program.modules {
        let metadata = modules
            .get(module.name.as_str())
            .ok_or("request module metadata differs from its IR graph")?;
        if metadata.package != module.package {
            return Err(format!(
                "module package ownership differs from its IR for `{}`",
                module.name
            ));
        }
    }
    if modules.values().any(|m| {
        m.role != ModuleRole::External && !program.modules.iter().any(|ir| ir.name == m.name)
    }) {
        return Err("request module metadata differs from its IR graph".into());
    }
    Ok(modules)
}

pub(super) fn roots(
    request: &BuildRequest,
    modules: &BTreeMap<&str, &ModuleInput>,
) -> Result<(), String> {
    for entry in &request.entries {
        if !modules.contains_key(entry.symbol.module.as_str()) {
            return Err("entry references a module outside the graph".into());
        }
    }
    for root in &request.test_roots {
        if !modules.contains_key(root.module.as_str()) {
            return Err("test root references a module outside the graph".into());
        }
    }
    for root in &request.root_handlers {
        if !modules.contains_key(root.service.module.as_str())
            || !modules.contains_key(root.handler.module.as_str())
        {
            return Err("root handler references a module outside the graph".into());
        }
    }
    Ok(())
}
