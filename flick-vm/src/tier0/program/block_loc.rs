// Lifetime-free `BlockLoc` addressing and suspended machine snapshot for
// cooperative single-threaded stepping.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BranchPath {
    bits: u64,
    len: u16,
    overflow: Vec<bool>,
}

impl BranchPath {
    fn push(&mut self, take_then: bool) {
        if self.len < 64 {
            if take_then {
                self.bits |= 1u64 << self.len;
            }
            self.len += 1;
        } else {
            self.overflow.push(take_then);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlockLoc {
    code: CodeId,
    path: BranchPath,
}

impl BlockLoc {
    fn root(code: CodeId) -> Self {
        Self {
            code,
            path: BranchPath::default(),
        }
    }

    fn push_branch(&mut self, take_then: bool) {
        self.path.push(take_then);
    }

    fn resolve<'p>(&self, program: &'p Program) -> Result<&'p Block, Error> {
        let mut block = &program
            .functions
            .get(&self.code)
            .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(self.code)))?
            .body;
        for idx in 0..self.path.len {
            let take_then = ((self.path.bits >> idx) & 1) != 0;
            let Exit::Branch { then_, else_, .. } = &block.exit else {
                return Err(Error::InvalidProgram(memory::Error::UnknownCode(self.code)));
            };
            block = if take_then { then_ } else { else_ };
        }
        for &take_then in &self.path.overflow {
            let Exit::Branch { then_, else_, .. } = &block.exit else {
                return Err(Error::InvalidProgram(memory::Error::UnknownCode(self.code)));
            };
            block = if take_then { then_ } else { else_ };
        }
        Ok(block)
    }
}

enum SliceExit {
    Completed(LanguageExit),
    Yielded,
}

/// Phase tracking for cooperative session evaluations (direct `main` protocol
/// vs REPL phrase auto-await with installed `:handle` wrappers).
enum EvalCompletionPhase {
    Direct,
    PhraseAwait {
        handlers: Vec<(Value, SessionHandlerKind)>,
        synthetic_codes: SessionSyntheticCodes,
        awaiting: bool,
    },
}

/// Owned snapshot of a suspended `Machine` between cooperative `step(fuel)` slices.
///
/// Holds all execution-local frames, fibers, scopes, handlers, and persistent
/// roots without borrowing `Heap` or `Program`.
pub(crate) struct SuspendedSessionEval {
    execution: usize,
    globals: BTreeMap<Name, Option<Value>>,
    cells: Vec<Value>,
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
    scratch: Vec<Value>,
    pending: Option<Value>,
    failure: Option<CauseToken>,
    failure_elected: bool,
    recoveries: BTreeMap<usize, RecoveryState>,
    next_recovery: usize,
    pending_handler: Option<Value>,
    top: Option<Value>,
    stdout: String,
    stderr: String,
    program_args: Vec<String>,
    host_services: HostServices,
    fuel: u64,
    main_stage: MainStage,
    root: Option<usize>,
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
    foreign: ForeignState,
    external_interrupt: Option<Arc<AtomicBool>>,
    sandbox: SandboxRuntime,
    phase: EvalCompletionPhase,
    pub(crate) rollback_module: Option<String>,
    prev_alloc_module: Option<u32>,
}

impl SuspendedSessionEval {
    pub(crate) fn stdout(&self) -> &str {
        &self.stdout
    }

    pub(crate) fn stderr(&self) -> &str {
        &self.stderr
    }
}

impl<'p, 'h> Machine<'p, 'h> {
    fn into_suspended(
        self,
        phase: EvalCompletionPhase,
        rollback_module: Option<String>,
        prev_alloc_module: Option<u32>,
    ) -> SuspendedSessionEval {
        SuspendedSessionEval {
            execution: self.execution,
            globals: self.globals,
            cells: self.cells,
            lazies: self.lazies,
            owners: self.owners,
            current_owner: self.current_owner,
            current_evidence: self.current_evidence,
            frames: self.frames,
            activations: self.activations,
            providers: self.providers,
            segments: self.segments,
            continuations: self.continuations,
            generators: self.generators,
            invocations: self.invocations,
            discard: self.discard,
            ready: self.ready,
            next_handler: self.next_handler,
            arguments: self.arguments,
            scratch: self.scratch,
            pending: self.pending,
            failure: self.failure,
            failure_elected: self.failure_elected,
            recoveries: self.recoveries,
            next_recovery: self.next_recovery,
            pending_handler: self.pending_handler,
            top: self.top,
            stdout: self.stdout,
            stderr: self.stderr,
            program_args: self.program_args,
            host_services: self.host_services,
            fuel: self.fuel,
            main_stage: self.main_stage,
            root: self.root,
            active_fiber: self.active_fiber,
            next_fiber: self.next_fiber,
            quantum: self.quantum,
            parked_fibers: self.parked_fibers,
            fiber_table: self.fiber_table,
            run_queue: self.run_queue,
            virtual_now_ns: self.virtual_now_ns,
            test_clock_mode: self.test_clock_mode,
            test_clock_waiter: self.test_clock_waiter,
            timers: self.timers,
            next_timer: self.next_timer,
            deferreds: self.deferreds,
            next_deferred: self.next_deferred,
            coords: self.coords,
            next_coord: self.next_coord,
            fiber_groups: self.fiber_groups,
            next_group: self.next_group,
            memo_maps: self.memo_maps,
            next_memo: self.next_memo,
            cached_cells: self.cached_cells,
            next_cached: self.next_cached,
            isaac_rngs: self.isaac_rngs,
            next_rng: self.next_rng,
            closing_scopes: self.closing_scopes,
            pending_provides: self.pending_provides,
            next_provide: self.next_provide,
            pending_resume_close: self.pending_resume_close,
            pending_provide_result: self.pending_provide_result,
            pending_layer_build: self.pending_layer_build,
            inherited_services: self.inherited_services,
            inherited_tail_ops: self.inherited_tail_ops,
            lazy_waiters: self.lazy_waiters,
            fiber_refs: self.fiber_refs,
            mask_depth: self.mask_depth,
            interrupt_requested: self.interrupt_requested,
            seeded_random: self.seeded_random,
            root_defects: self.root_defects,
            runtime_fibers_started: self.runtime_fibers_started,
            host_mode: self.host_mode,
            foreign: self.foreign,
            external_interrupt: self.external_interrupt,
            sandbox: self.sandbox,
            phase,
            rollback_module,
            prev_alloc_module,
        }
    }

    fn from_suspended(
        heap: &'h mut Heap,
        program: &'p Program,
        host_callbacks: Option<&'h mut (dyn HostCallbacks + 'static)>,
        s: SuspendedSessionEval,
    ) -> (Self, EvalCompletionPhase, Option<String>, Option<u32>) {
        let machine = Self {
            heap,
            program,
            execution: s.execution,
            globals: s.globals,
            cells: s.cells,
            lazies: s.lazies,
            owners: s.owners,
            current_owner: s.current_owner,
            current_evidence: s.current_evidence,
            frames: s.frames,
            activations: s.activations,
            providers: s.providers,
            segments: s.segments,
            continuations: s.continuations,
            generators: s.generators,
            invocations: s.invocations,
            discard: s.discard,
            ready: s.ready,
            next_handler: s.next_handler,
            arguments: s.arguments,
            scratch: s.scratch,
            pending: s.pending,
            failure: s.failure,
            failure_elected: s.failure_elected,
            recoveries: s.recoveries,
            next_recovery: s.next_recovery,
            pending_handler: s.pending_handler,
            top: s.top,
            stdout: s.stdout,
            stderr: s.stderr,
            program_args: s.program_args,
            host_services: s.host_services,
            fuel: s.fuel,
            main_stage: s.main_stage,
            root: s.root,
            active_fiber: s.active_fiber,
            next_fiber: s.next_fiber,
            quantum: s.quantum,
            parked_fibers: s.parked_fibers,
            fiber_table: s.fiber_table,
            run_queue: s.run_queue,
            virtual_now_ns: s.virtual_now_ns,
            test_clock_mode: s.test_clock_mode,
            test_clock_waiter: s.test_clock_waiter,
            timers: s.timers,
            next_timer: s.next_timer,
            deferreds: s.deferreds,
            next_deferred: s.next_deferred,
            coords: s.coords,
            next_coord: s.next_coord,
            fiber_groups: s.fiber_groups,
            next_group: s.next_group,
            memo_maps: s.memo_maps,
            next_memo: s.next_memo,
            cached_cells: s.cached_cells,
            next_cached: s.next_cached,
            isaac_rngs: s.isaac_rngs,
            next_rng: s.next_rng,
            closing_scopes: s.closing_scopes,
            pending_provides: s.pending_provides,
            next_provide: s.next_provide,
            pending_resume_close: s.pending_resume_close,
            pending_provide_result: s.pending_provide_result,
            pending_layer_build: s.pending_layer_build,
            inherited_services: s.inherited_services,
            inherited_tail_ops: s.inherited_tail_ops,
            lazy_waiters: s.lazy_waiters,
            fiber_refs: s.fiber_refs,
            mask_depth: s.mask_depth,
            interrupt_requested: s.interrupt_requested,
            seeded_random: s.seeded_random,
            root_defects: s.root_defects,
            runtime_fibers_started: s.runtime_fibers_started,
            host_mode: s.host_mode,
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            bridge: None,
            host_callbacks,
            foreign: s.foreign,
            external_interrupt: s.external_interrupt,
            sandbox: s.sandbox,
        };
        (machine, s.phase, s.rollback_module, s.prev_alloc_module)
    }
}
