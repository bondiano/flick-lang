// Stage 4A owner, scope-closing, and machine initialization structures.

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
        entry_owner: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClosePhase {
    Hooks,
    Children,
    PostHooks,
    Finalizers,
}

enum ScopeOutcome {
    Normal {
        binding: Option<Local>,
        value: Value,
    },
    Failure(CauseToken),
    Discard(Box<Discard>),
    ChildClose {
        parent: Box<ClosingScopeState>,
        inherited_exit: Option<CauseToken>,
    },
}

struct ClosingScopeState {
    owner: usize,
    previous: usize,
    phase: ClosePhase,
    remaining: Vec<Finalizer>,
    outcome: ScopeOutcome,
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

/// Runs the main apply/await protocol using the chosen [`HostMode`].
pub fn execute_main_program_outcome_with_host(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
) -> ExecutionReport {
    execute_main_program_outcome_with_args(heap, program, entry, fuel, host_mode, Vec::new())
}

/// Runs the main apply/await protocol with forwarded CLI `program_args`.
pub fn execute_main_program_outcome_with_args(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> ExecutionReport {
    execute_with_host_and_args(
        heap,
        program,
        entry,
        vec![],
        fuel,
        MainStage::Function,
        host_mode,
        program_args,
    )
}

/// [`execute_rooted_main_program_outcome`] with the chosen [`HostMode`].
pub fn execute_rooted_main_program_outcome_with_host(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: &Name,
    fuel: u64,
    host_mode: HostMode,
) -> ExecutionReport {
    execute_rooted_main_program_outcome_with_args(
        heap,
        program,
        entry,
        root,
        fuel,
        host_mode,
        Vec::new(),
    )
}

/// [`execute_rooted_main_program_outcome`] with forwarded CLI `program_args`.
pub fn execute_rooted_main_program_outcome_with_args(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: &Name,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> ExecutionReport {
    if !program.globals.iter().any(|global| &global.name == entry) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    execute_with_host_and_args(
        heap,
        program,
        root,
        vec![],
        fuel,
        MainStage::Root {
            main: entry.clone(),
            eager_inits: VecDeque::new(),
        },
        host_mode,
        program_args,
    )
}

/// Runs the main apply/await protocol with an explicit [`HostBridge`].
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub fn execute_main_program_outcome_with_bridge(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
    bridge: &mut HostBridge,
) -> ExecutionReport {
    execute_with_host_and_bridge(
        heap,
        program,
        entry,
        vec![],
        fuel,
        MainStage::Function,
        host_mode,
        Some(bridge),
    )
}

/// [`execute_rooted_main_program_outcome`] with an explicit [`HostBridge`].
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub fn execute_rooted_main_program_outcome_with_bridge(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: &Name,
    fuel: u64,
    host_mode: HostMode,
    bridge: &mut HostBridge,
) -> ExecutionReport {
    execute_main_with_bridge_and_inits(
        heap,
        program,
        entry,
        Some(root),
        &[],
        fuel,
        host_mode,
        bridge,
    )
}

/// Executes `entry` with optional root layer, optional entry-module eager
/// top-level initializers, and an explicit [`HostBridge`].
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
#[expect(clippy::too_many_arguments, reason = "explicit bridge main runner")]
pub fn execute_main_with_bridge_and_inits(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    eager_inits: &[Name],
    fuel: u64,
    host_mode: HostMode,
    bridge: &mut HostBridge,
) -> ExecutionReport {
    if !program.globals.iter().any(|global| &global.name == entry) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let mut pending: VecDeque<Name> = eager_inits
        .iter()
        .filter(|name| program.globals.iter().any(|g| &g.name == *name))
        .cloned()
        .collect();
    if let Some(root) = root {
        execute_with_host_and_bridge(
            heap,
            program,
            root,
            vec![],
            fuel,
            MainStage::Root {
                main: entry.clone(),
                eager_inits: pending,
            },
            host_mode,
            Some(bridge),
        )
    } else if let Some(first) = pending.pop_front() {
        execute_with_host_and_bridge(
            heap,
            program,
            &first,
            vec![],
            fuel,
            MainStage::EagerInit {
                pending,
                main: entry.clone(),
            },
            host_mode,
            Some(bridge),
        )
    } else {
        execute_with_host_and_bridge(
            heap,
            program,
            entry,
            vec![],
            fuel,
            MainStage::Function,
            host_mode,
            Some(bridge),
        )
    }
}

fn execute_with_host(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
    main_stage: MainStage,
    host_mode: HostMode,
) -> ExecutionReport {
    execute_with_host_and_args(
        heap,
        program,
        entry,
        arguments,
        fuel,
        main_stage,
        host_mode,
        Vec::new(),
    )
}

#[expect(clippy::too_many_arguments, reason = "internal execution entrypoint")]
fn execute_with_host_and_args(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
    main_stage: MainStage,
    host_mode: HostMode,
    program_args: Vec<String>,
) -> ExecutionReport {
    let arguments = match prepare_arguments(heap, program, entry, arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(error),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let execution = NEXT_EXECUTION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("fewer than usize::MAX model executions");
    let mut machine = Machine::new(heap, program, execution, arguments, fuel, main_stage);
    machine.host_mode = host_mode;
    machine.program_args = program_args;
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
#[expect(clippy::too_many_arguments, reason = "internal execution entrypoint")]
fn execute_with_host_and_bridge(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    fuel: u64,
    main_stage: MainStage,
    host_mode: HostMode,
    bridge: Option<&mut HostBridge>,
) -> ExecutionReport {
    let arguments = match prepare_arguments(heap, program, entry, arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(error),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let execution = NEXT_EXECUTION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("fewer than usize::MAX model executions");
    let mut machine = Machine::new(heap, program, execution, arguments, fuel, main_stage);
    machine.host_mode = host_mode;
    machine.bridge = bridge;
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
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
            stderr: String::new(),
            program_args: Vec::new(),
            host_services: HostServices::new(),
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
            host_mode: HostMode::Deterministic,
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            bridge: None,
            host_callbacks: None,
            foreign: ForeignState::default(),
            external_interrupt: None,
            sandbox: SandboxRuntime::default(),
        }
    }
}
