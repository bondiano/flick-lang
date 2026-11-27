//! Loading [`VmImage`]s into a live [`VmSession`] and executing standalone images.

use std::collections::BTreeMap;

use flick_core::iface::{self, Interface};
use flick_core::ir::memory::{CodeId, Name};

use super::save::canonical_closed_interface;
use super::{ImageError, VmImage, compiler_hash, memory_ir_hash};
use crate::session::VmSession;
use crate::tier0::{
    ExecutionReport, Heap, HostMode, SessionPersistentState, Value, allocate_execution_id,
    execute_main_with_persistent_state, execute_program_with_persistent_state,
};

/// Summary returned by [`load_image_into_session`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadReport {
    pub loaded_modules: Vec<String>,
    pub loaded_globals: Vec<Name>,
    pub restored_values: usize,
}

/// Validates hashes and imported interfaces, remaps `CodeId`s and `ObjectId`s
/// monotonically, and links `image` into `session`.
pub fn load_image_into_session(
    session: &mut VmSession,
    image: &VmImage,
) -> Result<LoadReport, ImageError> {
    verify_hashes(image)?;

    let already_loaded = session.loaded_modules();
    for module in &image.manifest.modules {
        if already_loaded.contains(module) {
            return Err(ImageError::ModuleAlreadyLoaded(module.clone()));
        }
    }

    // Verify all external imported modules exist in `session` with identical interfaces.
    let first_module = image
        .manifest
        .modules
        .first()
        .cloned()
        .unwrap_or_else(|| "<image>".to_owned());
    for (dep, expected_iface) in &image.manifest.external_imports {
        if !already_loaded.contains(dep) {
            return Err(ImageError::MissingDependency {
                module: first_module.clone(),
                dependency: dep.clone(),
            });
        }
        let live_iface = canonical_closed_interface(session.modules.get(dep), dep)?;
        check_interfaces_identical(dep, expected_iface, &live_iface)?;
    }

    // Validate heap snapshot indices before mutating `session`.
    for entry in &image.globals {
        if let Some(val) = &entry.value {
            image.heap.validate_value_indices(val)?;
        }
    }
    image.heap.validate_structure(&image.program.functions)?;

    // Merge program with monotonic CodeId remapping.
    let code_map = session
        .program
        .load_with_owners(&image.program, Some(&image.manifest.code_owners));

    let roots: Vec<Option<&super::heap_types::SnapValue>> =
        image.globals.iter().map(|g| g.value.as_ref()).collect();
    let default_module = image.manifest.modules.first().map(String::as_str);

    let restored = match image.heap.restore(
        &mut session.heap,
        session.execution,
        &code_map,
        &roots,
        default_module,
    ) {
        Ok(restored) => restored,
        Err(err) => {
            // Roll back the program additions on any restoration error.
            for module in &image.manifest.modules {
                let _ = session.program.unload_module(module);
            }
            let live_codes = session.heap.live_code_ids();
            session.program.reclaim_retired_codes(&live_codes);
            return Err(err);
        }
    };

    let mut loaded_globals = Vec::with_capacity(image.globals.len());
    let mut restored_values = 0usize;
    for (entry, maybe_val) in image.globals.iter().zip(restored.values) {
        loaded_globals.push(entry.name.clone());
        if let Some(val) = maybe_val {
            session.persistent.globals.insert(entry.name.clone(), val);
            restored_values += 1;
        }
    }
    session.persistent.cells.extend(restored.cells);
    session.persistent.lazies.extend(restored.lazies);

    for rec in &image.interfaces {
        session
            .modules
            .insert(rec.name.clone(), rec.interface.clone());
    }
    if session.root_layer_name.is_none()
        && let Some(root) = &image.manifest.root_layer
    {
        session.root_layer_name = Some(root.clone());
    }

    Ok(LoadReport {
        loaded_modules: image.manifest.modules.clone(),
        loaded_globals,
        restored_values,
    })
}

/// Executes `entry` in a standalone [`VmImage`] using the `main` protocol on
/// tier 0, restoring any pre-initialized globals/heap snapshot before running.
pub fn execute_image_main_with_args(
    heap: &mut Heap,
    image: &VmImage,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> Result<ExecutionReport, ImageError> {
    verify_hashes(image)?;
    let execution = allocate_execution_id();
    let persistent = restore_standalone_persistent(heap, image, execution)?;
    Ok(execute_main_with_persistent_state(
        heap,
        &image.program,
        execution,
        entry,
        image.root_layer(),
        persistent,
        fuel,
        host_mode,
        program_args,
    ))
}

/// Executes `entry` with `arguments` in a standalone [`VmImage`] on tier 0,
/// restoring any pre-initialized globals/heap snapshot before running.
pub fn execute_image_program(
    heap: &mut Heap,
    image: &VmImage,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
    host_mode: HostMode,
) -> Result<ExecutionReport, ImageError> {
    verify_hashes(image)?;
    let execution = allocate_execution_id();
    let persistent = restore_standalone_persistent(heap, image, execution)?;
    Ok(execute_program_with_persistent_state(
        heap,
        &image.program,
        execution,
        entry,
        arguments,
        persistent,
        fuel,
        host_mode,
    ))
}

fn restore_standalone_persistent(
    heap: &mut Heap,
    image: &VmImage,
    execution: usize,
) -> Result<SessionPersistentState, ImageError> {
    if !image.has_initialized_globals() && image.heap.objects.is_empty() {
        return Ok(SessionPersistentState::default());
    }
    let identity_codes: BTreeMap<CodeId, CodeId> =
        image.program.functions.keys().map(|&id| (id, id)).collect();
    let roots: Vec<Option<&super::heap_types::SnapValue>> =
        image.globals.iter().map(|g| g.value.as_ref()).collect();
    let default_module = image.manifest.modules.first().map(String::as_str);
    let restored = image
        .heap
        .restore(heap, execution, &identity_codes, &roots, default_module)?;
    let mut globals = BTreeMap::new();
    for (entry, maybe_val) in image.globals.iter().zip(restored.values) {
        if let Some(val) = maybe_val {
            globals.insert(entry.name.clone(), val);
        }
    }
    Ok(SessionPersistentState {
        globals,
        cells: restored.cells,
        lazies: restored.lazies,
    })
}

fn verify_hashes(image: &VmImage) -> Result<(), ImageError> {
    let expected_compiler = compiler_hash();
    if image.compiler_hash != expected_compiler {
        return Err(ImageError::CompilerHashMismatch {
            expected: expected_compiler,
            found: image.compiler_hash,
        });
    }
    let expected_ir = memory_ir_hash();
    if image.memory_ir_hash != expected_ir {
        return Err(ImageError::MemoryIrHashMismatch {
            expected: expected_ir,
            found: image.memory_ir_hash,
        });
    }
    Ok(())
}

fn check_interfaces_identical(
    dependency: &str,
    expected: &Interface,
    found: &Interface,
) -> Result<(), ImageError> {
    let exp_fp = iface::fingerprint(expected).map_err(|detail| ImageError::OpenInterface {
        module: dependency.to_owned(),
        detail,
    })?;
    let found_fp = iface::fingerprint(found).map_err(|detail| ImageError::OpenInterface {
        module: dependency.to_owned(),
        detail,
    })?;
    if exp_fp != found_fp || canonicalize(expected) != canonicalize(found) {
        return Err(ImageError::InterfaceMismatch {
            dependency: dependency.to_owned(),
            expected: exp_fp,
            found: found_fp,
        });
    }
    Ok(())
}

fn canonicalize(iface: &Interface) -> Interface {
    let mut bare = iface.clone();
    bare.dependencies.clear();
    bare.source = None;
    bare.docs.clear();
    #[cfg(not(target_arch = "wasm32"))]
    bare.local_schemes.clear();
    for export in &mut bare.exports {
        export.at = None;
    }
    bare
}
