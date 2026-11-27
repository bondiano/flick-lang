//! Live module unloading and per-module heap inventory on [`VmSession`].

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use flick_core::ir::memory::Name;

use super::ImageError;
use crate::session::VmSession;
use crate::tier0::{Data, LazyState};

/// Per-module count of currently live slots on a session's counted [`crate::tier0::Heap`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeapInventory {
    pub by_module: BTreeMap<String, usize>,
    pub unattributed: usize,
    pub total: usize,
}

impl HeapInventory {
    /// Returns the number of live heap slots currently attributed to `module`.
    #[must_use]
    pub fn live_slots_for(&self, module: &str) -> usize {
        self.by_module.get(module).copied().unwrap_or(0)
    }
}

/// Report returned by [`unload_module`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloadReport {
    /// The unloaded module name.
    pub module: String,
    /// Currently loaded modules that depended on `module`.
    pub dependents: Vec<String>,
    /// Global table entries removed for `module`.
    pub removed_globals: Vec<Name>,
    /// Heap slots attributed to `module` that remain alive because live
    /// closures or values in other modules still reference them.
    pub remaining_live_slots: usize,
}

/// Returns the live heap-slot inventory grouped by owning module.
#[must_use]
pub fn session_heap_inventory(session: &VmSession) -> HeapInventory {
    let (by_module, unattributed, total) = session
        .heap
        .module_slot_counts(session.program.code_owner());
    HeapInventory {
        by_module,
        unattributed,
        total,
    }
}

/// Unloads `module` from `session`:
///
/// 1. Finds all currently loaded modules that depended on `module`.
/// 2. Removes `module`'s exported interface and global table entries.
/// 3. Releases `module`'s global root values via exact reference counting so
///    code and values still referenced by live closures stay alive until the
///    last reference drops.
/// 4. Reclaims any retired root-arena cells/lazies and retired `CodeId`s that
///    are no longer referenced by any remaining live closure or root.
pub fn unload_module(session: &mut VmSession, module: &str) -> Result<UnloadReport, ImageError> {
    let loaded = session.loaded_modules();
    if !loaded.contains(module) {
        return Err(ImageError::UnknownModule(module.to_owned()));
    }

    // Identify every other currently loaded module that depended on `module`.
    let mut dep_set: BTreeSet<String> = BTreeSet::new();
    for other in &loaded {
        if other == module {
            continue;
        }
        let iface_dep = session
            .modules
            .get(other)
            .is_some_and(|iface| iface.dependencies.contains_key(module));
        let ir_dep = session
            .program
            .module_global_dependencies(other)
            .contains(module);
        if iface_dep || ir_dep {
            dep_set.insert(other.clone());
        }
    }
    let dependents: Vec<String> = dep_set.into_iter().collect();

    // Remove exported interface and global table entries.
    session.modules.remove(module);
    let removed_globals = session.program.unload_module(module);

    // Drop module's cached global values via exact RC.
    let to_remove: Vec<Name> = session
        .persistent
        .globals
        .keys()
        .filter(|name| name.module == module)
        .cloned()
        .collect();
    for name in to_remove {
        if let Some(val) = session.persistent.globals.remove(&name) {
            session
                .heap
                .drop_value(val)
                .map_err(|err| ImageError::Vm(err.to_string()))?;
        }
    }

    drop_module_runtime_roots(session, module)?;
    retire_module_cells_and_lazies(session, module);
    reclaim_unloaded(session)?;

    let remaining_live_slots = session_heap_inventory(session).live_slots_for(module);
    Ok(UnloadReport {
        module: module.to_owned(),
        dependents,
        removed_globals,
        remaining_live_slots,
    })
}

fn drop_module_runtime_roots(session: &mut VmSession, module: &str) -> Result<(), ImageError> {
    let mut dropped_values = Vec::new();
    let old_handlers = mem::take(&mut session.handlers);
    for h in old_handlers {
        if h.global.module == module {
            dropped_values.push(h.closure);
        } else {
            session.handlers.push(h);
        }
    }
    let old_layers = mem::take(&mut session.layers);
    for l in old_layers {
        if l.global.module == module {
            dropped_values.push(l.value);
        } else {
            session.layers.push(l);
        }
    }
    if session
        .root_layer_name
        .as_ref()
        .is_some_and(|n| n.module == module)
    {
        session.root_layer_name = None;
        if let Some(root) = session.root_layer.take() {
            dropped_values.push(root);
        }
    }
    if !dropped_values.is_empty() {
        session
            .heap
            .drop_values(dropped_values)
            .map_err(|err| ImageError::Vm(err.to_string()))?;
    }
    Ok(())
}

fn retire_module_cells_and_lazies(session: &mut VmSession, module: &str) {
    let old_cells = mem::take(&mut session.persistent.cells);
    for cell in old_cells {
        let owned_by_module = cell
            .object_id()
            .and_then(|id| session.heap.object_owner_module(id))
            == Some(module);
        if owned_by_module {
            session.retired_cells.push((module.to_owned(), cell));
        } else {
            session.persistent.cells.push(cell);
        }
    }
    let old_lazies = mem::take(&mut session.persistent.lazies);
    for lazy in old_lazies {
        let owned_by_module = lazy
            .object_id()
            .and_then(|id| session.heap.object_owner_module(id))
            == Some(module);
        if owned_by_module {
            session.retired_lazies.push((module.to_owned(), lazy));
        } else {
            session.persistent.lazies.push(lazy);
        }
    }
}

/// Reclaims retired root-arena cells/lazies and retired `CodeId`s belonging to
/// previously unloaded modules once no live session root or closure references them.
pub fn reclaim_unloaded(session: &mut VmSession) -> Result<(), ImageError> {
    if !session.retired_cells.is_empty() || !session.retired_lazies.is_empty() {
        let active_roots = session
            .persistent
            .globals
            .values()
            .chain(session.persistent.cells.iter())
            .chain(session.persistent.lazies.iter())
            .chain(session.root_layer.iter())
            .chain(session.layers.iter().map(|l| &l.value))
            .chain(session.handlers.iter().map(|h| &h.closure));
        let reachable = session.heap.reachable_cell_and_lazy_ids(active_roots);

        let mut dead_cells = Vec::new();
        let old_retired_cells = mem::take(&mut session.retired_cells);
        for (m, cell) in old_retired_cells {
            if cell.object_id().is_some_and(|id| reachable.contains(&id)) {
                session.retired_cells.push((m, cell));
            } else {
                dead_cells.push(cell);
            }
        }

        let mut dead_lazies = Vec::new();
        let old_retired_lazies = mem::take(&mut session.retired_lazies);
        for (m, lazy) in old_retired_lazies {
            if lazy.object_id().is_some_and(|id| reachable.contains(&id)) {
                session.retired_lazies.push((m, lazy));
            } else {
                dead_lazies.push(lazy);
            }
        }

        if !dead_cells.is_empty() || !dead_lazies.is_empty() {
            let mut detached = Vec::new();
            for cell in &dead_cells {
                if let Some(id) = cell.object_id()
                    && let Ok(obj) = session.heap.object_mut(id)
                    && let Data::Cell { contents, .. } = &mut obj.data
                    && let Some(v) = contents.take()
                {
                    detached.push(v);
                }
            }
            for lazy in &dead_lazies {
                if let Some(id) = lazy.object_id()
                    && let Ok(obj) = session.heap.object_mut(id)
                    && let Data::Lazy { state, .. } = &mut obj.data
                    && let Some(prev) = state.take()
                {
                    match prev {
                        LazyState::Pending(v)
                        | LazyState::Running { task: v, .. }
                        | LazyState::RunningLocal { task: Some(v) }
                        | LazyState::DoneOk(v) => detached.push(v),
                        LazyState::DoneCause(c) => detached.push(c.into_value()),
                        LazyState::RunningLocal { task: None } => {}
                    }
                }
            }
            let mut to_drop =
                Vec::with_capacity(dead_cells.len() + dead_lazies.len() + detached.len());
            to_drop.extend(dead_cells);
            to_drop.extend(dead_lazies);
            to_drop.extend(detached);
            session
                .heap
                .drop_values(to_drop)
                .map_err(|err| ImageError::Vm(err.to_string()))?;
        }
    }

    let live_codes = session.heap.live_code_ids();
    session.program.reclaim_retired_codes(&live_codes);
    Ok(())
}
