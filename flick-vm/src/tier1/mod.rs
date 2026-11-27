//! Register bytecode VM (tier 1) for Flick's ownership IR (`flick-ir/src/memory/`).
//!
//! Tier 1 compiles reachable memory-IR functions to a flat register bytecode
//! with intra-block peephole fusion and executes them directly over
//! [`crate::tier0::Heap`] and [`crate::stdlib`]. It supports sequences and
//! generators, tasks, cooperative single-threaded fibers, structured
//! concurrency, dynamic layers and contexts, arenas, and full C0-C6 scope
//! closing while reusing [`crate::stdlib::runtime`] and [`crate::tier0::Heap`].
//!
//! Any reachable operation outside tier 1's subset (such as `ForeignCall` or
//! `HostPrimitive::ProcessSpawn`) fails [`prepare`] with a structured
//! [`PrepareError`] naming the operation and its source span, with no silent
//! fallback to tier 0.

mod machine;
mod machine_arenas;
mod machine_async_ends;
mod machine_call;
mod machine_cleanup;
mod machine_concurrency;
mod machine_continuations;
mod machine_coords;
mod machine_coords_done;
mod machine_deferred;
mod machine_dict;
mod machine_fibers;
mod machine_fibers_wake;
mod machine_generators;
mod machine_groups;
mod machine_groups_stop;
mod machine_handlers;
mod machine_layers;
mod machine_layers_build;
mod machine_layers_merge;
mod machine_lazy;
mod machine_manual_scope;
mod machine_ops;
mod machine_race;
mod machine_recovery;
mod machine_resources;
mod machine_scheduler;
mod machine_scope_close;
mod machine_scopes;
mod machine_state;
mod machine_step_control;
mod machine_unwind;
mod machine_unwind_ends;
pub(crate) mod opcode;
mod prepare;
mod prepare_fuse;
mod prepare_ops;
mod prepare_ops_async;
mod prepare_ops_data;
mod prepare_ops_handlers;

use flick_core::ir::memory::{Name, Program};

pub use opcode::{PrepareError, Prepared, PreparedStats};
pub use prepare::{prepare, prepare_rooted};

use crate::sandbox::verify_bytecode;
use crate::tier0::{Error, Executed, ExecutionReport, Heap, HostMode, ProgramOutcome, Value};

/// Prepares `program` for `entry` and executes it with caller-supplied
/// `arguments` on `heap`, returning the full [`ExecutionReport`].
///
/// # Errors
/// Returns [`PrepareError`] if `program` is invalid, `entry` is unknown, or
/// any reachable operation is unsupported on tier 1.
pub fn execute_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
) -> Result<ExecutionReport, PrepareError> {
    let image = prepare(program, entry)?;
    Ok(execute_prepared_outcome(
        heap,
        &image,
        arguments,
        fuel,
        HostMode::Deterministic,
    ))
}

/// Prepares `program` for `entry`, executes it with `arguments`, and requires
/// a normal returned value.
///
/// # Errors
/// Returns [`PrepareError`] on preparation failure; runtime failures are
/// returned inside the inner `Result<Executed, Error>`.
pub fn execute_program(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
) -> Result<Result<Executed, Error>, PrepareError> {
    let report = execute_program_outcome(heap, program, entry, arguments, fuel)?;
    Ok(success_only(heap, report))
}

/// Prepares `program` for `entry` and runs the `main` protocol (initializing
/// `entry`, optionally applying a returned closure to `Unit`, then optionally
/// awaiting a returned task once) using [`HostMode::Deterministic`].
///
/// # Errors
/// Returns [`PrepareError`] if `program` fails tier-1 preparation.
pub fn execute_main_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
) -> Result<ExecutionReport, PrepareError> {
    execute_main_program_outcome_with_host(heap, program, entry, fuel, HostMode::Deterministic)
}

/// Prepares `program` for `entry` and runs the `main` protocol under `host_mode`.
///
/// # Errors
/// Returns [`PrepareError`] if `program` fails tier-1 preparation.
pub fn execute_main_program_outcome_with_host(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
) -> Result<ExecutionReport, PrepareError> {
    execute_main_program_outcome_with_args(heap, program, entry, fuel, host_mode, Vec::new())
}

/// Prepares `program` for `entry` and runs the `main` protocol under `host_mode`
/// with forwarded CLI `program_args`.
///
/// # Errors
/// Returns [`PrepareError`] if `program` fails tier-1 preparation.
pub fn execute_main_program_outcome_with_args(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> Result<ExecutionReport, PrepareError> {
    let image = prepare(program, entry)?;
    Ok(machine::execute_prepared(
        heap,
        &image,
        Vec::new(),
        fuel,
        host_mode,
        program_args,
        true,
    ))
}

/// Prepares `program` for `entry` with root service layer `root` (`Σ₀`) and
/// runs the `main` protocol using [`HostMode::Deterministic`].
///
/// # Errors
/// Returns [`PrepareError`] if `program` fails tier-1 preparation.
pub fn execute_rooted_main_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: &Name,
    fuel: u64,
) -> Result<ExecutionReport, PrepareError> {
    execute_rooted_main_program_outcome_with_args(
        heap,
        program,
        entry,
        root,
        fuel,
        HostMode::Deterministic,
        Vec::new(),
    )
}

/// Prepares `program` for `entry` with root service layer `root` (`Σ₀`) and
/// runs the `main` protocol under `host_mode` with forwarded `program_args`.
///
/// # Errors
/// Returns [`PrepareError`] if `program` fails tier-1 preparation.
pub fn execute_rooted_main_program_outcome_with_args(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: &Name,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> Result<ExecutionReport, PrepareError> {
    let image = prepare_rooted(program, entry, root)?;
    Ok(machine::execute_prepared(
        heap,
        &image,
        Vec::new(),
        fuel,
        host_mode,
        program_args,
        true,
    ))
}

/// Prepares `program` for `entry`, runs the `main` protocol under
/// [`HostMode::Deterministic`], and requires a normal returned value.
///
/// # Errors
/// Returns [`PrepareError`] on preparation failure.
pub fn execute_main_program(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
) -> Result<Result<Executed, Error>, PrepareError> {
    let report = execute_main_program_outcome(heap, program, entry, fuel)?;
    Ok(success_only(heap, report))
}

/// Executes an already prepared tier-1 [`Prepared`] image with `arguments`.
pub fn execute_prepared_outcome(
    heap: &mut Heap,
    image: &Prepared,
    arguments: Vec<Value>,
    fuel: u64,
    host_mode: HostMode,
) -> ExecutionReport {
    if let Err(err) = verify_bytecode(image) {
        let _ = heap.drop_values(arguments);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::VerificationFailed(err.to_string())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    machine::execute_prepared(heap, image, arguments, fuel, host_mode, Vec::new(), false)
}

/// Executes an already prepared tier-1 [`Prepared`] image using the `main`
/// apply/await protocol.
pub fn execute_prepared_main_outcome_with_host(
    heap: &mut Heap,
    image: &Prepared,
    fuel: u64,
    host_mode: HostMode,
) -> ExecutionReport {
    if let Err(err) = verify_bytecode(image) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::VerificationFailed(err.to_string())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    machine::execute_prepared(heap, image, Vec::new(), fuel, host_mode, Vec::new(), true)
}

fn success_only(heap: &mut Heap, report: ExecutionReport) -> Result<Executed, Error> {
    match report.outcome {
        ProgramOutcome::Returned(value) => Ok(Executed {
            value,
            stdout: report.stdout,
        }),
        ProgramOutcome::Raised(value) => {
            heap.drop_value(value)?;
            Err(Error::UnhandledFailure)
        }
        ProgramOutcome::Failed(cause) => {
            let error = heap.cause_error(&cause);
            heap.drop_cause(cause)?;
            Err(error?)
        }
        ProgramOutcome::Fault(error) => Err(error),
    }
}
