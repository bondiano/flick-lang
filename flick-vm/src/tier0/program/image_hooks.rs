// Execution hooks for running restored VM images with pre-initialized globals,
// cells, and lazies while reusing `Machine`'s full teardown and main protocol.

/// Executes `entry` using the `main` protocol (optional root layer `root`,
/// optional apply to `Unit`, optional await) starting from pre-initialized
/// persistent state restored from a VM image. All persistent roots are owned
/// and released cleanly by `Machine::cleanup`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_main_with_persistent_state(
    heap: &mut Heap,
    program: &Program,
    execution: usize,
    entry: &Name,
    root: Option<&Name>,
    mut persistent: SessionPersistentState,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> ExecutionReport {
    if let Err(error) = memory::validate_program(program) {
        let _ = shutdown_persistent_state(heap, &mut persistent);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::InvalidProgram(error)),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    if !program.globals.iter().any(|g| &g.name == entry) {
        let _ = shutdown_persistent_state(heap, &mut persistent);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let (start_entry, main_stage) = if let Some(root_name) = root {
        if !program.globals.iter().any(|g| &g.name == root_name) {
            let _ = shutdown_persistent_state(heap, &mut persistent);
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(Error::UnknownGlobal(root_name.clone())),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
        (
            root_name,
            MainStage::Root {
                main: entry.clone(),
                eager_inits: VecDeque::new(),
            },
        )
    } else {
        (entry, MainStage::Function)
    };
    let mut machine = Machine::new(heap, program, execution, vec![], fuel, main_stage);
    machine.host_mode = host_mode;
    machine.program_args = program_args;
    machine.load_persistent(&mut persistent);
    let result = machine.global(start_entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

/// Executes `entry` with `arguments` starting from pre-initialized persistent
/// state restored from a VM image. All persistent roots are owned and released
/// cleanly by `Machine::cleanup`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_program_with_persistent_state(
    heap: &mut Heap,
    program: &Program,
    execution: usize,
    entry: &Name,
    arguments: Vec<Value>,
    mut persistent: SessionPersistentState,
    fuel: u64,
    host_mode: HostMode,
) -> ExecutionReport {
    let arguments = match prepare_arguments(heap, program, entry, arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            let _ = shutdown_persistent_state(heap, &mut persistent);
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(error),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let mut machine = Machine::new(heap, program, execution, arguments, fuel, MainStage::Done);
    machine.host_mode = host_mode;
    machine.load_persistent(&mut persistent);
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}
