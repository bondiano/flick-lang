//! Persistent-session bytecode preparation, incremental hot-swap extension,
//! and session execution hooks for tier 1.

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use flick_core::ir::memory::{self, CodeId, HostPrimitive, Intrinsic, Name, Program};

use crate::sandbox::verify_bytecode;
use crate::tier0::{
    Error, ExecutionReport, Heap, HostMode, ProgramOutcome, SessionPersistentState, Value,
};

use super::machine::{GlobalState, Machine, RegVal};
use super::opcode::{PrepareError, Prepared, PreparedOp};
use super::prepare::{Interner, Lowering};

fn make_prepared_ops(ops: Vec<(String, String)>) -> Vec<PreparedOp> {
    ops.into_iter()
        .map(|(effect, op)| {
            let console_fallback = if effect == "Host.Console" {
                match op.as_str() {
                    "stdout_line" => Some(Intrinsic::PrintLine),
                    "stdout_write" => Some(Intrinsic::Host(HostPrimitive::StdoutWrite)),
                    "stderr_line" => Some(Intrinsic::Host(HostPrimitive::StderrLine)),
                    _ => None,
                }
            } else {
                None
            };
            PreparedOp {
                effect,
                op,
                console_fallback,
            }
        })
        .collect()
}

fn interner_from_vec<T: Ord + Clone>(values: Vec<T>) -> Interner<T> {
    let entries = values
        .iter()
        .enumerate()
        .map(|(idx, v)| (v.clone(), idx))
        .collect();
    Interner { entries, values }
}

/// Validates `program` and prepares a tier-1 register bytecode image for
/// `entry`, optional `root`, and any `extra_codes` (such as live heap closures
/// created by earlier session phrases).
///
/// # Errors
/// Returns [`PrepareError`] if `program` is invalid, an entry is missing, or
/// any reachable operation is unsupported on tier 1.
pub(crate) fn prepare_session(
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    extra_codes: &BTreeSet<CodeId>,
) -> Result<Prepared, PrepareError> {
    memory::validate_program(program).map_err(PrepareError::Invalid)?;
    let initializers: BTreeMap<_, _> = program
        .globals
        .iter()
        .map(|global| (global.name.clone(), global.initializer))
        .collect();
    if !initializers.contains_key(entry) {
        return Err(PrepareError::UnknownEntry(entry.clone()));
    }
    if let Some(root_name) = root
        && !initializers.contains_key(root_name)
    {
        return Err(PrepareError::UnknownEntry(root_name.clone()));
    }
    let mut lowering = Lowering {
        program,
        initializers,
        codes: Interner::default(),
        globals: Interner::default(),
        tags: Interner::default(),
        dictionary_fields: Interner::default(),
        loops: Interner::default(),
        operations: Interner::default(),
    };
    let entry_id = lowering.intern_global(entry);
    let root_id = root.map(|root_name| lowering.intern_global(root_name));
    for &code in extra_codes {
        if program.functions.contains_key(&code) {
            lowering.codes.intern(code);
        }
    }
    let mut functions = Vec::new();
    while let Some(&code) = lowering.codes.values.get(functions.len()) {
        functions.push(lowering.prepare_function(code, &program.functions[&code])?);
    }
    let globals: Vec<usize> = lowering
        .globals
        .values
        .iter()
        .map(|name| lowering.codes.entries[&lowering.initializers[name]])
        .collect();
    Ok(Prepared {
        functions,
        ir_codes: lowering.codes.values,
        code_indices: lowering.codes.entries,
        globals,
        global_names: lowering.globals.values,
        entry: entry_id,
        root: root_id,
        tags: lowering.tags.values,
        dictionary_fields: lowering.dictionary_fields.values,
        loops: lowering.loops.values,
        operations: make_prepared_ops(lowering.operations.values),
    })
}

/// Extends an already-prepared tier-1 image in place after a hot swap while
/// preserving every existing function index, instruction offset, and table
/// index so suspended frames continue executing old code untouched.
///
/// Newly swapped globals have their initializer code indices updated and all
/// newly reachable replacement functions are compiled, fused, appended, and
/// verified through [`verify_bytecode`].
pub(crate) fn extend_prepared(
    prepared: &mut Prepared,
    program: &Program,
    target_globals: &[Name],
    extra_codes: &BTreeSet<CodeId>,
) -> Result<(), PrepareError> {
    memory::validate_program(program).map_err(PrepareError::Invalid)?;
    let initializers: BTreeMap<_, _> = program
        .globals
        .iter()
        .map(|global| (global.name.clone(), global.initializer))
        .collect();
    for name in &prepared.global_names {
        if !initializers.contains_key(name) {
            return Err(PrepareError::UnknownEntry(name.clone()));
        }
    }
    for name in target_globals {
        if !initializers.contains_key(name) {
            return Err(PrepareError::UnknownEntry(name.clone()));
        }
    }

    let op_pairs: Vec<(String, String)> = prepared
        .operations
        .iter()
        .map(|op| (op.effect.clone(), op.op.clone()))
        .collect();
    let mut lowering = Lowering {
        program,
        initializers,
        codes: interner_from_vec(prepared.ir_codes.clone()),
        globals: interner_from_vec(prepared.global_names.clone()),
        tags: interner_from_vec(prepared.tags.clone()),
        dictionary_fields: interner_from_vec(prepared.dictionary_fields.clone()),
        loops: interner_from_vec(prepared.loops.clone()),
        operations: interner_from_vec(op_pairs),
    };

    for name in &prepared.global_names {
        lowering.codes.intern(lowering.initializers[name]);
    }
    for name in target_globals {
        lowering.intern_global(name);
    }
    for &code in extra_codes {
        if program.functions.contains_key(&code) {
            lowering.codes.intern(code);
        }
    }

    let mut functions = prepared.functions.clone();
    while let Some(&code) = lowering.codes.values.get(functions.len()) {
        functions.push(lowering.prepare_function(code, &program.functions[&code])?);
    }
    let globals: Vec<usize> = lowering
        .globals
        .values
        .iter()
        .map(|name| lowering.codes.entries[&lowering.initializers[name]])
        .collect();

    let candidate = Prepared {
        functions,
        ir_codes: lowering.codes.values,
        code_indices: lowering.codes.entries,
        globals,
        global_names: lowering.globals.values,
        entry: prepared.entry,
        root: prepared.root,
        tags: lowering.tags.values,
        dictionary_fields: lowering.dictionary_fields.values,
        loops: lowering.loops.values,
        operations: make_prepared_ops(lowering.operations.values),
    };
    if let Err(err) = verify_bytecode(&candidate) {
        return Err(PrepareError::Unsupported {
            code: 0,
            operation: format!("bytecode verification failed: {err}"),
            span: None,
        });
    }
    *prepared = candidate;
    Ok(())
}

pub(super) fn load_persistent_into_machine(
    machine: &mut Machine<'_, '_>,
    persistent: &mut SessionPersistentState,
) {
    let image = machine.image;
    machine
        .globals
        .resize_with(image.globals.len(), || GlobalState::Empty);
    for (idx, name) in image.global_names.iter().enumerate() {
        if let Some(val) = persistent.globals.remove(name) {
            machine.globals[idx] = GlobalState::Ready(RegVal::Value(val));
        } else {
            machine.globals[idx] = GlobalState::Empty;
        }
    }
    machine.cells = mem::take(&mut persistent.cells);
    machine.lazies = mem::take(&mut persistent.lazies);
}

pub(super) fn save_persistent_from_machine(
    machine: &mut Machine<'_, '_>,
    persistent: &mut SessionPersistentState,
) {
    let globals = mem::take(&mut machine.globals);
    for (idx, state) in globals.into_iter().enumerate() {
        if let GlobalState::Ready(reg) = state {
            let val = machine.materialize_reg(reg);
            let name = machine.image.global_names[idx].clone();
            persistent.globals.insert(name, val);
        }
    }
    persistent.cells = mem::take(&mut machine.cells);
    persistent.lazies = mem::take(&mut machine.lazies);
}

/// Executes a verified tier-1 [`Prepared`] image against persistent session
/// globals, cells, and lazies, with optional unary `arguments`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_session_prepared(
    heap: &mut Heap,
    image: &Prepared,
    execution: usize,
    persistent: &mut SessionPersistentState,
    entry: &Name,
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
    let Some(entry_id) = image.global_names.iter().position(|n| n == entry) else {
        let _ = heap.drop_values(arguments);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    };
    if let Err(err) = heap.ensure_owned(&arguments) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(err),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    if let Err(err) = heap.admit_values(&arguments) {
        let _ = heap.drop_values(arguments);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(err),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let mut machine = Machine::new(heap, image, arguments, fuel, host_mode, Vec::new(), false);
    machine.execution = execution;
    load_persistent_into_machine(&mut machine, persistent);
    let result = machine
        .start_global_top(entry_id)
        .and_then(|()| machine.run());
    let outcome = match result.and_then(|exit| machine.report_exit(exit)) {
        Ok(outcome) => outcome,
        Err(error) => ProgramOutcome::Fault(error),
    };
    save_persistent_from_machine(&mut machine, persistent);
    let outcome = if let Err(error) = machine.cleanup() {
        let _ = outcome.release(machine.heap);
        ProgramOutcome::Fault(error)
    } else {
        outcome
    };
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}
