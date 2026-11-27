//! Saving whole sessions, selected module subsets, and static AOT builds into [`VmImage`].

use std::collections::{BTreeMap, BTreeSet};

use flick_core::iface::{self, Interface};
use flick_core::ir::memory::{self, CodeId, Name, Program};

use super::format::{cbor_encode, sha256_bytes};
use super::heap_snap::HeapSnapper;
use super::heap_types::{HeapSnapshot, SnapValue};
use super::{
    GlobalEntry, ImageError, ImageManifest, ModuleRecord, PreparedMetadata, VmImage, compiler_hash,
    memory_ir_hash,
};
use crate::session::VmSession;
use crate::session::program::collect_block_codes;
use crate::tier1;

const TIER1_METADATA_VERSION: u32 = 1;

/// Selects which modules of a [`VmSession`] to capture in an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveSelection<'a> {
    /// Save every module currently loaded in the session.
    WholeSession,
    /// Save only the named modules and their reachable globals/heap state.
    Modules(&'a [&'a str]),
}

/// Saves the entire live [`VmSession`] into a [`VmImage`].
pub fn save_session_image(session: &VmSession, entry: Option<Name>) -> Result<VmImage, ImageError> {
    save_session_modules(session, SaveSelection::WholeSession, entry)
}

/// Saves a selected set of modules (or the whole session) from a live [`VmSession`].
///
/// Rejects open interfaces and any reachable host resource or transient handle,
/// naming the root global and exact field/index path on failure.
pub fn save_session_modules(
    session: &VmSession,
    selection: SaveSelection<'_>,
    entry: Option<Name>,
) -> Result<VmImage, ImageError> {
    let loaded = session.loaded_modules();
    let selected: BTreeSet<String> = match selection {
        SaveSelection::WholeSession => loaded,
        SaveSelection::Modules(names) => {
            let mut set = BTreeSet::new();
            for &name in names {
                if !loaded.contains(name) {
                    return Err(ImageError::UnknownModule(name.to_owned()));
                }
                set.insert(name.to_owned());
            }
            set
        }
    };

    // Snapshot all initialized globals belonging to the selected modules first,
    // collecting every `CodeId` referenced by live closures/tasks/generators.
    let mut snapper = HeapSnapper::new(&session.heap);
    let mut snapped_globals: BTreeMap<Name, SnapValue> = BTreeMap::new();
    for (name, val) in &session.persistent.globals {
        if selected.contains(&name.module) {
            let root_val = snapper.snapshot_root(name, val)?;
            snapped_globals.insert(name.clone(), root_val);
        }
    }
    let extra_codes = snapper.referenced_codes.clone();
    let mut heap = snapper.finish();

    // Extract a compact, self-contained memory-IR slice covering selected
    // globals and all code reachable from them and from live heap closures.
    let (program, live_to_image, code_owners) =
        session.program.extract_slice(&selected, &extra_codes);
    heap.remap_codes(&live_to_image)?;

    // Discover external module dependencies and verify closed interfaces.
    let mut external_imports: BTreeMap<String, Interface> = BTreeMap::new();
    let mut interfaces: Vec<ModuleRecord> = Vec::with_capacity(selected.len());
    for mod_name in &selected {
        let iface = canonical_closed_interface(session.modules.get(mod_name), mod_name)?;
        let mut dep_names: BTreeSet<String> = iface.dependencies.keys().cloned().collect();
        dep_names.extend(session.program.module_global_dependencies(mod_name));
        let mut import_fingerprints = BTreeMap::new();
        for dep in dep_names {
            if selected.contains(&dep) {
                continue;
            }
            let dep_iface = canonical_closed_interface(session.modules.get(&dep), &dep)?;
            let fp =
                iface::fingerprint(&dep_iface).map_err(|detail| ImageError::OpenInterface {
                    module: dep.clone(),
                    detail,
                })?;
            import_fingerprints.insert(dep.clone(), fp);
            external_imports.entry(dep).or_insert(dep_iface);
        }
        interfaces.push(ModuleRecord {
            name: mod_name.clone(),
            interface: iface,
            import_fingerprints,
        });
    }

    let globals: Vec<GlobalEntry> = program
        .globals
        .iter()
        .map(|g| GlobalEntry {
            name: g.name.clone(),
            initializer: g.initializer,
            value: snapped_globals.remove(&g.name),
        })
        .collect();

    let root_layer = session
        .root_layer_name
        .as_ref()
        .filter(|n| selected.contains(&n.module))
        .cloned();

    let prepared = if external_imports.is_empty() && globals.iter().all(|g| g.value.is_none()) {
        entry
            .as_ref()
            .and_then(|e| try_prepare_metadata(&program, e))
    } else {
        None
    };

    let manifest = ImageManifest {
        entry,
        root_layer,
        modules: selected.into_iter().collect(),
        external_imports,
        code_owners,
    };

    Ok(VmImage {
        compiler_hash: compiler_hash(),
        memory_ir_hash: memory_ir_hash(),
        manifest,
        interfaces,
        program,
        prepared,
        globals,
        heap,
    })
}

/// Builds a cold (uninitialized-heap) standalone [`VmImage`] from compiled
/// memory IR and closed module interfaces (used by `flick build --target vm`).
pub fn build_static_image(
    program: Program,
    raw_interfaces: Vec<Interface>,
    entry: Option<Name>,
    root_layer: Option<Name>,
) -> Result<VmImage, ImageError> {
    memory::validate_program(&program)
        .map_err(|err| ImageError::Vm(format!("invalid memory IR: {err}")))?;

    let mut iface_by_name: BTreeMap<String, Interface> = BTreeMap::new();
    for raw in raw_interfaces {
        let name = raw.module.clone();
        let closed = sanitize_closed_interface(raw)?;
        iface_by_name.insert(name, closed);
    }

    let mut modules_set: BTreeSet<String> = iface_by_name.keys().cloned().collect();
    for g in &program.globals {
        modules_set.insert(g.name.module.clone());
    }

    let mut interfaces = Vec::with_capacity(modules_set.len());
    for name in &modules_set {
        let iface = match iface_by_name.remove(name) {
            Some(i) => i,
            None => Interface {
                module: name.clone(),
                ..Interface::default()
            },
        };
        interfaces.push(ModuleRecord {
            name: name.clone(),
            interface: iface,
            import_fingerprints: BTreeMap::new(),
        });
    }

    let code_owners = infer_static_code_owners(&program);
    let globals: Vec<GlobalEntry> = program
        .globals
        .iter()
        .map(|g| GlobalEntry {
            name: g.name.clone(),
            initializer: g.initializer,
            value: None,
        })
        .collect();

    let prepared = entry
        .as_ref()
        .and_then(|e| try_prepare_metadata(&program, e));

    let manifest = ImageManifest {
        entry,
        root_layer,
        modules: modules_set.into_iter().collect(),
        external_imports: BTreeMap::new(),
        code_owners,
    };

    Ok(VmImage {
        compiler_hash: compiler_hash(),
        memory_ir_hash: memory_ir_hash(),
        manifest,
        interfaces,
        program,
        prepared,
        globals,
        heap: HeapSnapshot::default(),
    })
}

fn try_prepare_metadata(program: &Program, entry: &Name) -> Option<PreparedMetadata> {
    let prepared = tier1::prepare(program, entry).ok()?;
    let stats = prepared.stats();
    let ir_bytes = cbor_encode(program, "memory-ir").ok()?;
    Some(PreparedMetadata {
        entry: entry.clone(),
        tier1_version: TIER1_METADATA_VERSION,
        functions: stats.functions,
        instructions: stats.instructions,
        fused_compute: stats.fused_compute,
        fused_compare_branch: stats.fused_compare_branch,
        fused_update_cell: stats.fused_update_cell,
        fused_match_flag: stats.fused_match_flag,
        fused_tail_calls: stats.fused_tail_calls,
        memory_ir_digest: sha256_bytes(&ir_bytes),
    })
}

pub(crate) fn canonical_closed_interface(
    registered: Option<&Interface>,
    module: &str,
) -> Result<Interface, ImageError> {
    let raw = match registered {
        Some(iface) => iface.clone(),
        None => Interface {
            module: module.to_owned(),
            ..Interface::default()
        },
    };
    sanitize_closed_interface(raw)
}

fn sanitize_closed_interface(mut iface: Interface) -> Result<Interface, ImageError> {
    let module = iface.module.clone();
    #[cfg(not(target_arch = "wasm32"))]
    iface.local_schemes.clear();
    // `iface::fingerprint` enforces closedness via `flick_core::iface::closed`.
    let _ = iface::fingerprint(&iface)
        .map_err(|detail| ImageError::OpenInterface { module, detail })?;
    Ok(iface)
}

fn infer_static_code_owners(program: &Program) -> BTreeMap<CodeId, String> {
    let mut owners = BTreeMap::new();
    for global in &program.globals {
        let mut work = vec![global.initializer];
        while let Some(id) = work.pop() {
            if owners.contains_key(&id) {
                continue;
            }
            owners.insert(id, global.name.module.clone());
            if let Some(func) = program.functions.get(&id) {
                let mut refs = BTreeSet::new();
                collect_block_codes(&func.body, &mut refs);
                work.extend(refs);
            }
        }
    }
    owners
}
