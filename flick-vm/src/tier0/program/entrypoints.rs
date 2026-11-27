// Public one-shot execution entrypoints and argument validation for `tier0::program`.

/// The result token remains owned by the caller after all execution roots
/// are released. A returned closure is safely droppable, but cannot be
/// invoked by a different execution: code identities are execution-local.
#[derive(Debug)]
pub struct Executed {
    pub value: Value,
    pub stdout: String,
}

/// An executed outcome owns exactly one token on return or typed failure.
#[derive(Debug)]
pub enum ProgramOutcome {
    Returned(Value),
    Raised(Value),
    Failed(CauseToken),
    Fault(Error),
}

impl ProgramOutcome {
    /// Releases precisely this outcome's owned root, without sweeping the heap.
    pub fn release(self, heap: &mut Heap) -> Result<(), Error> {
        match self {
            Self::Returned(value) | Self::Raised(value) => heap.drop_value(value),
            Self::Failed(cause) => heap.drop_cause(cause),
            Self::Fault(_) => Ok(()),
        }
    }
}

#[derive(Debug)]
/// Captures output on every executed outcome, including faults and fuel exhaustion.
pub struct ExecutionReport {
    pub outcome: ProgramOutcome,
    pub stdout: String,
    pub stderr: String,
}

/// Evaluates with a caller-owned heap and transfers all valid argument tokens.
/// Returned and raised values remain caller-owned after execution cleanup.
/// Invalid/foreign ownership is rejected before token acceptance, as in
/// [`execute_program`]; this function never sweeps either heap.
pub fn execute_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
) -> ExecutionReport {
    execute(heap, program, entry, arguments, fuel, MainStage::Done)
}

/// Runs the main apply/await protocol, stopping immediately on typed failure.
pub fn execute_main_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
) -> ExecutionReport {
    execute(heap, program, entry, vec![], fuel, MainStage::Function)
}

/// [`execute_main_program_outcome`] with the root services `Σ₀`: the layer
/// the global `root` gives answers each operation no frame answers, for the
/// whole execution, and is released with its other roots.
pub fn execute_rooted_main_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: &Name,
    fuel: u64,
) -> ExecutionReport {
    // `execute` checks the root it starts from; `main` is checked here
    if !program.globals.iter().any(|global| &global.name == entry) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let stage = MainStage::Root {
        main: entry.clone(),
        eager_inits: VecDeque::new(),
    };
    execute(heap, program, root, vec![], fuel, stage)
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

static NEXT_EXECUTION: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn next_execution_id() -> usize {
    NEXT_EXECUTION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("fewer than usize::MAX VM executions")
}

/// Evaluates a global once, then applies the supplied unary arguments.
/// All accepted argument tokens are transferred into this execution.
/// Runtime failure and exhausted fuel release its real roots, including
/// pending operands and suspended frames; no heap reachability sweep runs.
/// External aliases and a successful returned value remain caller-owned.
pub fn execute_program(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
) -> Result<Executed, Error> {
    let report = execute_program_outcome(heap, program, entry, arguments, fuel);
    success_only(heap, report)
}

/// Runs the main protocol in one execution: optionally apply its ordinary
/// function to Unit, then optionally await the resulting task once. The
/// cell arena survives both stages and is closed only after the outcome.
pub fn execute_main_program(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
) -> Result<Executed, Error> {
    let report = execute_main_program_outcome(heap, program, entry, fuel);
    success_only(heap, report)
}

fn prepare_arguments(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
) -> Result<Vec<Value>, Error> {
    heap.ensure_owned(&arguments)?;
    if let Err(error) = heap.admit_values(&arguments) {
        heap.drop_values(arguments)?;
        return Err(error);
    }
    if let Err(error) = memory::validate_program(program) {
        heap.drop_values(arguments)?;
        return Err(Error::InvalidProgram(error));
    }
    if !program.globals.iter().any(|global| &global.name == entry) {
        heap.drop_values(arguments)?;
        return Err(Error::UnknownGlobal(entry.clone()));
    }
    Ok(arguments)
}

fn execute(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
    main_stage: MainStage,
) -> ExecutionReport {
    execute_with_host(
        heap,
        program,
        entry,
        arguments,
        fuel,
        main_stage,
        HostMode::Deterministic,
    )
}
