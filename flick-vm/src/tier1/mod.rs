//! Register bytecode VM (tier 1) for Flick's ownership IR (`flick-ir/src/memory/`).
//!
//! Tier 1 compiles reachable memory-IR functions to a flat register bytecode
//! with intra-block peephole fusion and executes them directly over
//! [`crate::tier0::Heap`] and [`crate::stdlib`]. All arithmetic, unary
//! operations, and standard library primitives delegate to [`crate::stdlib`],
//! and [`crate::tier0::Heap::finish`] enforces zero leaked slots identically
//! to tier 0.
//!
//! Any reachable operation outside tier 1's pure synchronous subset fails
//! [`prepare`] with a structured [`PrepareError`] naming the operation and its
//! source span, with no silent fallback to tier 0.

mod machine;
mod machine_call;
mod machine_dict;
mod machine_ops;
mod machine_recovery;
mod opcode;
mod prepare;
mod prepare_fuse;
mod prepare_ops;
mod prepare_ops_data;

use flick_core::ir::memory::{Name, Program};

pub use opcode::{PrepareError, Prepared, PreparedStats};
pub use prepare::prepare;

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
/// `entry` and applying a returned closure to `Unit` once) using
/// [`HostMode::Deterministic`].
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
    machine::execute_prepared(heap, image, arguments, fuel, host_mode, Vec::new(), false)
}

/// Executes an already prepared tier-1 [`Prepared`] image using the `main`
/// apply-to-`Unit` protocol.
pub fn execute_prepared_main_outcome_with_host(
    heap: &mut Heap,
    image: &Prepared,
    fuel: u64,
    host_mode: HostMode,
) -> ExecutionReport {
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
