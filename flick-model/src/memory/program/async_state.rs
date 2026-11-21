// Stage 4A owner, scope-closing, and machine initialization structures.

const DEFAULT_VIRTUAL_NOW_NS: u64 = 1_750_000_000_000_000_000;

#[derive(Clone, Copy, Debug)]
enum LayerBuildTarget {
    Local(Local),
    Top,
    Provide(usize),
}

struct PendingProvide {
    owner: usize,
    previous: usize,
    body_thunk: Value,
    saved_evidence: Option<OwnerEvidence>,
    prev_memo: Option<Value>,
    binding: Local,
}

enum PendingProvideResult {
    Ready(usize, Value),
    Failed(usize, CauseToken),
}

enum OwnerHook {
    StopFiberGroup(usize),
}

enum Finalizer {
    Task {
        task: Value,
        services: Vec<Value>,
    },
    WithCause {
        func: Value,
        services: Vec<Value>,
    },
    WithExit {
        func: Value,
        services: Vec<Value>,
    },
    Release {
        value: Value,
        release_fn: Value,
        services: Vec<Value>,
    },
    CloseChildOwner(usize),
    ReleaseMemo {
        memo_id: usize,
        layer_id: ObjectId,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClosePhase {
    Hooks,
    Children,
    PostHooks,
    Finalizers,
}

enum ScopeOutcome<'p> {
    Normal {
        binding: Option<Local>,
        value: Value,
    },
    Failure(CauseToken),
    Discard(Box<Discard<'p>>),
    ChildClose {
        parent: Box<ClosingScopeState<'p>>,
        inherited_exit: Option<CauseToken>,
    },
}

struct ClosingScopeState<'p> {
    owner: usize,
    previous: usize,
    phase: ClosePhase,
    remaining: Vec<Finalizer>,
    outcome: ScopeOutcome<'p>,
    exit_override: Option<CauseToken>,
    finalizer_failure: Option<CauseToken>,
    elect_on_fail: bool,
    saved_mask: usize,
    masked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnerPhase {
    Open,
    Closing,
    Finalizing,
    Closed,
}

struct OwnerState {
    phase: OwnerPhase,
    exit_cause: Option<CauseToken>,
    hooks: VecDeque<OwnerHook>,
    children: BTreeSet<usize>,
    watchers: usize,
    close_waiter: Option<usize>,
    finalizers: Vec<Finalizer>,
    /// Sole counted owners, never a reachability-derived disposal set.
    arenas: Vec<Value>,
}

impl OwnerState {
    fn is_open(&self) -> bool {
        self.phase == OwnerPhase::Open
    }

    fn is_closing_or_closed(&self) -> bool {
        self.phase != OwnerPhase::Open
    }

    fn is_finalizing(&self) -> bool {
        self.phase == OwnerPhase::Finalizing
    }

    fn is_finalizing_or_closed(&self) -> bool {
        matches!(self.phase, OwnerPhase::Finalizing | OwnerPhase::Closed)
    }

    fn is_closed(&self) -> bool {
        self.phase == OwnerPhase::Closed
    }
}

impl Default for OwnerState {
    fn default() -> Self {
        Self {
            phase: OwnerPhase::Open,
            exit_cause: None,
            hooks: VecDeque::new(),
            children: BTreeSet::new(),
            watchers: 0,
            close_waiter: None,
            finalizers: Vec::new(),
            arenas: Vec::new(),
        }
    }
}

impl<'p, 'h> Machine<'p, 'h> {
    fn new(
        heap: &'h mut Heap,
        program: &'p Program,
        execution: usize,
        arguments: Vec<Value>,
        fuel: u64,
        main_stage: MainStage,
    ) -> Self {
        let mut fiber_table = BTreeMap::new();
        fiber_table.insert(1, FiberRecord::root());
        Self {
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
            active_fiber: 1,
            next_fiber: 2,
            quantum: FIBER_QUANTUM,
            parked_fibers: BTreeMap::new(),
            fiber_table,
            run_queue: VecDeque::new(),
            virtual_now_ns: DEFAULT_VIRTUAL_NOW_NS,
            test_clock_mode: false,
            test_clock_waiter: None,
            timers: BTreeMap::new(),
            next_timer: 0,
            deferreds: BTreeMap::new(),
            next_deferred: 0,
            coords: BTreeMap::new(),
            next_coord: 0,
            fiber_groups: BTreeMap::new(),
            next_group: 0,
            memo_maps: BTreeMap::new(),
            next_memo: 0,
            cached_cells: BTreeMap::new(),
            next_cached: 0,
            isaac_rngs: BTreeMap::new(),
            next_rng: 0,
            closing_scopes: BTreeMap::new(),
            pending_provides: BTreeMap::new(),
            next_provide: 0,
            pending_resume_close: None,
            pending_provide_result: None,
            pending_layer_build: None,
            inherited_services: Vec::new(),
            inherited_tail_ops: Vec::new(),
            lazy_waiters: BTreeMap::new(),
            fiber_refs: BTreeMap::new(),
            mask_depth: 0,
            interrupt_requested: None,
            seeded_random: None,
            root_defects: Vec::new(),
            runtime_fibers_started: 0,
        }
    }
}
