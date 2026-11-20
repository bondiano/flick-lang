//! One counted execution: explicit frames, global roots and error unwinding.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, mem};

use flick_core::ir::{Lit, Pat};
use flick_core::types::OpClass;

use flick_core::ir::memory::{
    self, Block, CodeId, Exit, Instruction, Intrinsic, Local, Name, Operation, Program,
};

use super::{
    ArenaHandle, ArenaView, CellHandle, Data, Error, ExitData, Heap, IteratorHandle, IteratorState,
    LazyHandle, LazyState, Locals, ObjectToken, OwnerHandle, SeqData, Value, VarHandle, patterns,
    peek, primitives, tagged_boundary, take,
};

#[path = "dictionaries.rs"]
mod dictionaries;
#[path = "evidence.rs"]
mod evidence;
#[path = "handlers.rs"]
mod handlers;
#[path = "layers.rs"]
mod layers;
use super::causes::StepFailure;
use super::{CauseToken, DefectSpec, LanguageExit, Reason, charge};
use layers::Provider;
#[path = "recovery.rs"]
mod recovery;
use recovery::RecoveryState;

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
        };
    }
    let stage = MainStage::Root(entry.clone());
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
    let arguments = match prepare_arguments(heap, program, entry, arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(error),
                stdout: String::new(),
            };
        }
    };
    let execution = NEXT_EXECUTION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("fewer than usize::MAX model executions");
    let mut machine = Machine {
        heap,
        program,
        execution,
        globals: BTreeMap::new(),
        cells: Vec::new(),
        lazies: Vec::new(),
        owners: vec![OwnerState::default()],
        current_owner: 0,
        current_evidence: None,
        frames: Vec::new(),
        activations: BTreeMap::new(),
        providers: BTreeMap::new(),
        segments: BTreeMap::new(),
        continuations: BTreeMap::new(),
        generators: BTreeMap::new(),
        invocations: BTreeMap::new(),
        discard: None,
        ready: None,
        next_handler: 0,
        arguments: arguments.into(),
        scratch: Vec::new(),
        pending: None,
        failure: None,
        failure_elected: false,
        recoveries: BTreeMap::new(),
        next_recovery: 0,
        pending_handler: None,
        top: None,
        stdout: String::new(),
        fuel,
        main_stage,
        root: None,
    };
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
    }
}

enum Destination<'p> {
    RecoverySelection {
        id: usize,
    },
    RecoveryHelper {
        id: usize,
        binding: Local,
    },
    RecoveryBody {
        binding: Local,
        owner: usize,
        evidence: Option<OwnerEvidence>,
    },
    LayerRun {
        provider: usize,
        binding: Local,
    },
    Handle {
        activation: usize,
        binding: Local,
    },
    Discontinue {
        cont_id: usize,
        binding: Local,
    },
    Invocation(usize),
    Catch {
        handler: Value,
        binding: Local,
    },
    CaptureExit {
        binding: Local,
    },
    CatchDefect {
        handler: Value,
        binding: Local,
    },
    Top,
    Bind(Local),
    Initialize {
        name: Name,
        binding: Option<Local>,
    },
    Modify {
        cell: CellHandle,
        binding: Local,
    },
    CloseScope {
        owner: usize,
        previous: usize,
        binding: Option<Local>,
    },
    ClosingScope {
        state: Box<ClosingScopeState<'p>>,
    },
    RestoreOwner {
        previous: usize,
        binding: Local,
    },
    Loop {
        id: String,
        code: CodeId,
        args: Vec<Value>,
        binding: Local,
    },
    FunctionBoundary {
        binding: Local,
    },
    GenerateStrict {
        items: Vec<Value>,
        remaining: Option<usize>,
        binding: Local,
    },
    GeneratorStep {
        gen_id: usize,
        binding: Local,
    },
    LazyComplete {
        lazy: LazyHandle,
        binding: Local,
    },
    AcquireRegister {
        owner: usize,
        release: Value,
        binding: Local,
    },
    BracketUse {
        use_fn: Value,
        release_fn: Value,
        binding: Local,
    },
    BracketRelease {
        resource: Value,
        release_fn: Value,
        binding: Local,
    },
    BracketFinish {
        outcome: BracketOutcome,
        binding: Local,
    },
    DiscardFinalizer,
}

enum BracketOutcome {
    Success(Value),
    Failure(CauseToken),
}

enum Finalizer {
    Task(Value),
    WithCause(Value),
    WithExit(Value),
    Release { value: Value, release_fn: Value },
}

enum ScopeOutcome<'p> {
    Normal {
        binding: Option<Local>,
        value: Value,
    },
    Failure(CauseToken),
    Discard(Box<Discard<'p>>),
}

struct ClosingScopeState<'p> {
    owner: usize,
    previous: usize,
    remaining: Vec<Finalizer>,
    outcome: ScopeOutcome<'p>,
    finalizer_failure: Option<CauseToken>,
}

#[derive(Clone, PartialEq, Eq)]
enum MainStage {
    /// The root layer is evaluated; `main`, named here, comes next.
    Root(Name),
    Function,
    Task,
    Done,
}

/// Copyable context metadata, never a copyable language ownership token.
#[derive(Clone, Copy)]
struct OwnerEvidence {
    heap: usize,
    execution: usize,
    index: usize,
}

impl OwnerEvidence {
    fn from_owner(owner: &OwnerHandle) -> Self {
        Self {
            heap: owner.heap,
            execution: owner.execution,
            index: owner.index,
        }
    }

    fn value(self) -> Value {
        Value::Owner(OwnerHandle {
            heap: self.heap,
            execution: self.execution,
            index: self.index,
        })
    }
}

struct Frame<'p> {
    recovery_id: Option<usize>,
    block: &'p Block,
    at: usize,
    locals: Locals,
    vars: Vec<Value>,
    iterators: Vec<Value>,
    destination: Destination<'p>,
    evidence: Option<OwnerEvidence>,
}

struct InstalledOperation {
    effect: String,
    name: String,
    class: OpClass,
    dicts: usize,
    arity: usize,
    function: Value,
}

struct Activation {
    operations: Vec<InstalledOperation>,
    ret: Option<Value>,
    errors: Option<Value>,
    owner: usize,
}

struct Segment<'p> {
    frames: Vec<Frame<'p>>,
    owner: usize,
    binding: Local,
}

struct CapturedContinuation<'p> {
    frames: Vec<Frame<'p>>,
    owner: usize,
    binding: Local,
}

enum GeneratorRun<'p> {
    Fresh {
        seq: Value,
        code: CodeId,
    },
    Suspended {
        frames: Vec<Frame<'p>>,
        owner: usize,
        yield_binding: Local,
    },
    Running,
    Done,
}

struct GeneratorSlot<'p> {
    run: GeneratorRun<'p>,
    remaining: Option<usize>,
}

enum InvocationEnd {
    Caller(Local),
    Tail(usize),
    Abort(Local),
    Control { cont_id: usize, binding: Local },
}

struct Invocation {
    callee: Option<Value>,
    arguments: VecDeque<Value>,
    end: InvocationEnd,
    evidence: Option<OwnerEvidence>,
}

enum TransferNext<'p> {
    Invoke(usize),
    Raise,
    LoopContinue {
        id: String,
        code: CodeId,
        args: Vec<Value>,
        binding: Local,
    },
    DeliverValue {
        binding: Local,
        value: Value,
    },
    ReturnNone {
        binding: Local,
    },
    ResumeRunning {
        saved_failure: Option<CauseToken>,
    },
    FinishGenerateStrict {
        binding: Local,
        items: Vec<Value>,
    },
    DeliverNone {
        binding: Local,
    },
    Chain(Box<Discard<'p>>),
}

struct Discard<'p> {
    frames: Vec<Frame<'p>>,
    owner: Option<usize>,
    next: TransferNext<'p>,
}

#[derive(Default)]
struct OwnerState {
    closing: bool,
    closed: bool,
    finalizers: Vec<Finalizer>,
    /// Sole counted owners, never a reachability-derived disposal set.
    arenas: Vec<Value>,
}

struct Machine<'p, 'h> {
    heap: &'h mut Heap,
    program: &'p Program,
    execution: usize,
    /// None marks an initializer currently on the frame stack.
    globals: BTreeMap<Name, Option<Value>>,
    /// The sole strong owners of actual cell allocations. User cell
    /// handles do not own contents, so cycles through them need no tracing.
    cells: Vec<Value>,
    /// The sole strong owners of root-owned single-flight `Lazy` cells.
    lazies: Vec<Value>,
    owners: Vec<OwnerState>,
    current_owner: usize,
    current_evidence: Option<OwnerEvidence>,
    frames: Vec<Frame<'p>>,
    activations: BTreeMap<usize, Activation>,
    providers: BTreeMap<usize, Provider>,
    segments: BTreeMap<usize, Segment<'p>>,
    continuations: BTreeMap<usize, Option<CapturedContinuation<'p>>>,
    generators: BTreeMap<usize, GeneratorSlot<'p>>,
    invocations: BTreeMap<usize, Invocation>,
    discard: Option<Discard<'p>>,
    ready: Option<usize>,
    next_handler: usize,
    arguments: VecDeque<Value>,
    /// Consuming operands remain explicit roots until transferred/dropped.
    scratch: Vec<Value>,
    /// A computed result stays rooted while its operands are released.
    pending: Option<Value>,
    failure: Option<CauseToken>,
    failure_elected: bool,
    recoveries: BTreeMap<usize, RecoveryState>,
    next_recovery: usize,
    /// Owns a handler while fallible application has not installed its frame.
    pending_handler: Option<Value>,
    top: Option<Value>,
    stdout: String,
    fuel: u64,
    main_stage: MainStage,
    /// The provider of `Σ₀`, which no frame holds.
    root: Option<usize>,
}

include!("program/driving.rs");
include!("program/returns.rs");
include!("program/calls.rs");
include!("program/values.rs");
include!("program/contexts.rs");
include!("program/scoped_values.rs");
include!("program/control_flow.rs");
include!("program/generators.rs");
include!("program/scope_finalizers.rs");
include!("program/frame_roots.rs");
include!("program/cleanup.rs");
include!("program/test_runner.rs");

#[cfg(test)]
mod tests {
    use super::super::{DefectCode, ObjectId, ObjectToken};
    use super::*;
    use flick_core::ir::Lit;
    use flick_core::opt::perceus;

    include!("program/tests/scope_modify.rs");
    include!("program/tests/boundary_roots.rs");
    include!("program/tests/unwind_defects.rs");
    include!("program/tests/arena_protocol.rs");
    include!("program/tests/compound_recovery.rs");
    include!("program/tests/recovery_integrity.rs");
}
