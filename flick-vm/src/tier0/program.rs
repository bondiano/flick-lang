//! One counted execution: explicit frames, global roots and error unwinding.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
use std::time::{Duration, Instant};
use std::{iter, mem};

use flick_core::ir::{BinOp, Lit, Pat};
use flick_core::types::OpClass;

use flick_core::ir::memory::{
    self, Block, CliIntrinsic, CodeId, Exit, HostPrimitive, HostServiceIntrinsic, Instruction,
    Intrinsic, Local, Name, Operation, Program,
};

#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
use super::HostBridge;
use super::{
    ArenaHandle, ArenaView, CellHandle, Data, Error, ExitData, Heap, HostMode, HostServices,
    IteratorHandle, IteratorState, LayerData, LazyHandle, LazyState, Locals, ObjectId, ObjectToken,
    OwnerHandle, SeqData, Value, VarHandle, patterns, peek, tagged_boundary, take,
};
use crate::stdlib::cli::{CliRunState, CliStep};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
use crate::stdlib::runtime::coop::HOST_CTX_KEY;
use crate::stdlib::runtime::coop::MEMO_REF_KEY;
use crate::stdlib::{
    self, DEFAULT_VIRTUAL_NOW_NS, FIBER_QUANTUM, FuelCounter, HostCallbacks, HostContext,
    QUANTUM_VIRTUAL_NS, primitives,
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
use layers::{MemoRestore, Provider};
#[path = "recovery.rs"]
mod recovery;
use recovery::RecoveryState;

include!("program/entrypoints.rs");

enum Destination {
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
        state: Box<ClosingScopeState>,
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
        saved_mask: usize,
        binding: Local,
    },
    DiscardCleanupDone {
        saved_mask: usize,
        masked: bool,
        paused: Box<Discard>,
    },
    OnExitDiscardRunFin {
        saved_mask: usize,
        masked: bool,
        paused: Box<Discard>,
    },
    RestoreMask {
        saved_mask: usize,
        check_on_exit: bool,
        next: Box<Destination>,
    },
    EnsuringFinish {
        finalizer: Value,
        binding: Local,
    },
    EnsuringDone {
        outcome: BracketOutcome,
        saved_mask: usize,
        binding: Local,
    },
    OnExitDecide {
        interruptible: bool,
        decide: Value,
        binding: Local,
    },
    OnExitRunFin {
        outcome: BracketOutcome,
        saved_mask: usize,
        binding: Local,
    },
    OnExitFinish {
        outcome: BracketOutcome,
        saved_mask: usize,
        binding: Local,
    },
    CatchCause {
        handler: Value,
        binding: Local,
    },
    OrDie {
        binding: Local,
    },
    TimedFinish {
        start_ns: u64,
        binding: Local,
    },
    CachedComplete {
        cell_id: usize,
        next: Box<Destination>,
    },
    ProvideContext {
        context: Value,
        binding: Local,
    },
    WithServices {
        services: Vec<Value>,
        next: Box<Destination>,
    },
    RestoreFiberRef {
        key: String,
        previous: Option<Value>,
        binding: Local,
    },
    RestoreOwnerEvidence {
        previous: usize,
        evidence: Option<OwnerEvidence>,
        binding: Local,
    },
    RestoreEnvRefs {
        previous: BTreeMap<String, Value>,
        binding: Local,
    },
    RestoreRandom {
        previous: Option<usize>,
        rng_id: usize,
        binding: Local,
    },
    LayerBuildFinish {
        inner_owner: usize,
        parent_scope: usize,
        prev_owner: usize,
        prev_ev: Option<OwnerEvidence>,
        prev_memo: Option<Value>,
        memo_entry: Option<(usize, ObjectId)>,
        or_die: bool,
        target: LayerBuildTarget,
    },
    #[cfg_attr(
        not(all(feature = "native-host", not(target_arch = "wasm32"))),
        allow(dead_code)
    )]
    RaiseFromCatch,
    CliStep {
        state: Box<CliRunState>,
        binding: Local,
    },
    #[cfg_attr(
        not(all(feature = "native-host", not(target_arch = "wasm32"))),
        allow(dead_code)
    )]
    HostSyncCallback {
        req_id: u64,
        remaining_args: VecDeque<Value>,
    },
    #[cfg_attr(
        not(all(feature = "native-host", not(target_arch = "wasm32"))),
        allow(dead_code)
    )]
    HostApplyChain {
        remaining_args: VecDeque<Value>,
    },
    #[cfg_attr(
        not(all(feature = "native-host", not(target_arch = "wasm32"))),
        allow(dead_code)
    )]
    ForeignLayerBuildFinish {
        or_die: bool,
        target: LayerBuildTarget,
    },
    DiscardFinalizer,
}

enum BracketOutcome {
    Success(Value),
    Failure(CauseToken),
}

#[derive(Clone, PartialEq, Eq)]
enum MainStage {
    /// The root layer is evaluated; `eager_inits` and `main`, named here, come next.
    Root {
        main: Name,
        eager_inits: VecDeque<Name>,
    },
    /// Entry-module top-level value bindings evaluated before `main`.
    EagerInit {
        pending: VecDeque<Name>,
        main: Name,
    },
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

struct Frame {
    recovery_id: Option<usize>,
    block: BlockLoc,
    at: usize,
    locals: Locals,
    vars: Vec<Value>,
    iterators: Vec<Value>,
    destination: Destination,
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

struct Segment {
    frames: Vec<Frame>,
    owner: usize,
    binding: Local,
}

struct CapturedContinuation {
    frames: Vec<Frame>,
    owner: usize,
    binding: Local,
}

enum GeneratorRun {
    Fresh {
        seq: Value,
        code: CodeId,
    },
    Suspended {
        frames: Vec<Frame>,
        owner: usize,
        yield_binding: Local,
    },
    Running,
    Done,
}

struct GeneratorSlot {
    run: GeneratorRun,
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

enum TransferNext {
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
        propagate_exit: bool,
    },
    FinishGenerateStrict {
        binding: Local,
        items: Vec<Value>,
    },
    DeliverNone {
        binding: Local,
    },
    FinishCutoff,
    Chain(Box<Discard>),
}

struct Discard {
    frames: Vec<Frame>,
    owner: Option<usize>,
    next: TransferNext,
    aborted: bool,
}

struct Machine<'p, 'h> {
    heap: &'h mut Heap,
    program: &'p Program,
    execution: usize,
    /// None marks an initializer currently on the frame stack.
    globals: BTreeMap<Name, Option<Value>>,
    /// The sole strong owners of actual cell allocations.
    cells: Vec<Value>,
    /// The sole strong owners of root-owned single-flight `Lazy` cells.
    lazies: Vec<Value>,
    owners: Vec<OwnerState>,
    current_owner: usize,
    current_evidence: Option<OwnerEvidence>,
    frames: Vec<Frame>,
    activations: BTreeMap<usize, Activation>,
    providers: BTreeMap<usize, Provider>,
    segments: BTreeMap<usize, Segment>,
    continuations: BTreeMap<usize, Option<CapturedContinuation>>,
    generators: BTreeMap<usize, GeneratorSlot>,
    invocations: BTreeMap<usize, Invocation>,
    discard: Option<Discard>,
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
    stderr: String,
    program_args: Vec<String>,
    host_services: HostServices,
    fuel: u64,
    main_stage: MainStage,
    /// The provider of `Σ₀`, which no frame holds.
    root: Option<usize>,
    // Stage 4A cooperative concurrency, virtual clock, and dynamic layers.
    active_fiber: usize,
    next_fiber: usize,
    quantum: u32,
    parked_fibers: BTreeMap<usize, FiberContext>,
    fiber_table: BTreeMap<usize, FiberRecord>,
    run_queue: VecDeque<usize>,
    virtual_now_ns: u64,
    test_clock_mode: bool,
    test_clock_waiter: Option<TestClockAdvance>,
    timers: BTreeMap<(u64, usize), TimerEntry>,
    next_timer: usize,
    deferreds: BTreeMap<usize, DeferredState>,
    next_deferred: usize,
    coords: BTreeMap<usize, CoordState>,
    next_coord: usize,
    fiber_groups: BTreeMap<usize, FiberGroupState>,
    next_group: usize,
    memo_maps: BTreeMap<usize, MemoMapState>,
    next_memo: usize,
    cached_cells: BTreeMap<usize, CachedCellState>,
    next_cached: usize,
    isaac_rngs: BTreeMap<usize, primitives::IsaacRng>,
    next_rng: usize,
    closing_scopes: BTreeMap<usize, Box<ClosingScopeState>>,
    pending_provides: BTreeMap<usize, PendingProvide>,
    next_provide: usize,
    pending_resume_close: Option<usize>,
    pending_provide_result: Option<PendingProvideResult>,
    pending_layer_build: Option<(Value, usize, usize)>,
    inherited_services: Vec<Value>,
    inherited_tail_ops: Vec<InstalledOperation>,
    lazy_waiters: BTreeMap<ObjectId, Vec<(usize, Local)>>,
    fiber_refs: BTreeMap<String, Value>,
    mask_depth: usize,
    interrupt_requested: Option<u64>,
    seeded_random: Option<usize>,
    root_defects: Vec<CauseToken>,
    runtime_fibers_started: u64,
    host_mode: HostMode,
    #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
    bridge: Option<&'h mut HostBridge>,
    host_callbacks: Option<&'h mut (dyn HostCallbacks + 'static)>,
    foreign: ForeignState,
    external_interrupt: Option<Arc<AtomicBool>>,
    sandbox: SandboxRuntime,
}

include!("program/async_state.rs");
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
include!("program/foreign.rs");
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
include!("program/foreign_calls.rs");
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
include!("program/foreign_cb.rs");
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
include!("program/foreign_poll.rs");
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
include!("program/foreign_layers.rs");
#[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
include!("program/foreign_stub.rs");
include!("program/block_loc.rs");
include!("program/stepper.rs");
include!("program/driving.rs");
include!("program/driving_unwind.rs");
include!("program/returns.rs");
include!("program/calls.rs");
include!("program/values.rs");
include!("program/contexts.rs");
include!("program/scoped_values.rs");
include!("program/control_flow.rs");
include!("program/generators.rs");
include!("program/scope_finalizers.rs");
include!("program/owner_close.rs");
include!("program/frame_roots.rs");
include!("program/cleanup.rs");
include!("program/fibers.rs");
include!("program/fibers_wake.rs");
include!("program/scheduler.rs");
include!("program/coords.rs");
include!("program/coords_race.rs");
include!("program/concurrency.rs");
include!("program/concurrency_par.rs");
include!("program/concurrency_race.rs");
include!("program/concurrency_def.rs");
include!("program/async_dests.rs");
include!("program/async_dests_unwind.rs");
include!("program/fiber_groups.rs");
include!("program/fiber_groups_stop.rs");
include!("program/context_ops.rs");
include!("program/manual_scope.rs");
include!("program/layer_build.rs");
include!("program/layer_merge.rs");
include!("program/test_runner.rs");
include!("program/session_hooks.rs");
include!("program/image_hooks.rs");
include!("program/sandbox_hooks.rs");

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
