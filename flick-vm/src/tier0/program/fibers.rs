// Cooperative single-threaded fiber contexts, spawn, park/yield, interrupt, and wake.

struct FiberContext {
    current_owner: usize,
    current_evidence: Option<OwnerEvidence>,
    frames: Vec<Frame>,
    discard: Option<Discard>,
    ready: Option<usize>,
    scratch: Vec<Value>,
    pending: Option<Value>,
    failure: Option<CauseToken>,
    failure_elected: bool,
    pending_handler: Option<Value>,
    top: Option<Value>,
    arguments: VecDeque<Value>,
    inherited_services: Vec<Value>,
    inherited_tail_ops: Vec<InstalledOperation>,
    fiber_refs: BTreeMap<String, Value>,
    mask_depth: usize,
    interrupt_requested: Option<u64>,
    seeded_random: Option<usize>,
    pending_resume_close: Option<usize>,
    pending_provide_result: Option<PendingProvideResult>,
    pending_layer_build: Option<(Value, usize, usize)>,
}

impl FiberContext {
    fn empty(owner: usize, evidence: Option<OwnerEvidence>) -> Self {
        Self {
            current_owner: owner,
            current_evidence: evidence,
            frames: Vec::new(),
            discard: None,
            ready: None,
            scratch: Vec::new(),
            pending: None,
            failure: None,
            failure_elected: false,
            pending_handler: None,
            top: None,
            arguments: VecDeque::new(),
            inherited_services: Vec::new(),
            inherited_tail_ops: Vec::new(),
            fiber_refs: BTreeMap::new(),
            mask_depth: 0,
            interrupt_requested: None,
            seeded_random: None,
            pending_resume_close: None,
            pending_provide_result: None,
            pending_layer_build: None,
        }
    }
}

enum FiberOutcome {
    Running,
    Success(Value),
    Failure(CauseToken),
}

#[derive(Clone, Copy)]
enum FiberWaitKind {
    Join(Local),
    Await(Local),
    Interrupt(Local),
    CoordDone { coord_id: usize, index: usize },
}

#[derive(Clone, Copy)]
struct FiberWaiter {
    fiber: usize,
    kind: FiberWaitKind,
}

struct SupervisorHook {
    owner: usize,
    callback: Value,
    services: Vec<Value>,
}

#[derive(Clone, Copy, Debug)]
enum ParkReason {
    Deferred { id: usize },
    Fiber { target: usize },
    Sleep { timer_id: usize },
    Never,
    Coord { coord_id: usize },
    MemoWait { target: LayerBuildTarget },
    OwnerChildren { owner: usize },
    OwnerHook { owner: usize },
    FiberGroupJoin,
    FiberGroupEmpty,
    TestClock,
    Lazy { lazy_id: ObjectId },
    #[cfg_attr(
        not(all(feature = "native-host", not(target_arch = "wasm32"))),
        allow(dead_code)
    )]
    ForeignAsync { call_id: u64 },
}

struct FiberRecord {
    owner: Option<usize>,
    outcome: FiberOutcome,
    parked: Option<ParkReason>,
    waiters: Vec<FiberWaiter>,
    supervisor: Option<SupervisorHook>,
    coord_parent: Option<(usize, usize)>,
    group_watchers: Vec<(usize, usize)>,
    is_supervisor_of: Option<usize>,
}

impl FiberRecord {
    fn root() -> Self {
        Self {
            owner: None,
            outcome: FiberOutcome::Running,
            parked: None,
            waiters: Vec::new(),
            supervisor: None,
            coord_parent: None,
            group_watchers: Vec::new(),
            is_supervisor_of: None,
        }
    }
}

enum TimerAction {
    WakeSleep { fiber: usize, binding: Local },
    FireTimeout { coord_id: usize },
}

struct TimerEntry {
    id: usize,
    deadline_ns: u64,
    action: TimerAction,
}

struct TestClockAdvance {
    caller_fiber: usize,
    binding: Local,
    target_ns: u64,
}

impl Machine<'_, '_> {
    fn swap_fiber_context(&mut self, ctx: &mut FiberContext) {
        mem::swap(&mut self.current_owner, &mut ctx.current_owner);
        mem::swap(&mut self.current_evidence, &mut ctx.current_evidence);
        mem::swap(&mut self.frames, &mut ctx.frames);
        mem::swap(&mut self.discard, &mut ctx.discard);
        mem::swap(&mut self.ready, &mut ctx.ready);
        mem::swap(&mut self.scratch, &mut ctx.scratch);
        mem::swap(&mut self.pending, &mut ctx.pending);
        mem::swap(&mut self.failure, &mut ctx.failure);
        mem::swap(&mut self.failure_elected, &mut ctx.failure_elected);
        mem::swap(&mut self.pending_handler, &mut ctx.pending_handler);
        mem::swap(&mut self.top, &mut ctx.top);
        mem::swap(&mut self.arguments, &mut ctx.arguments);
        mem::swap(&mut self.inherited_services, &mut ctx.inherited_services);
        mem::swap(&mut self.inherited_tail_ops, &mut ctx.inherited_tail_ops);
        mem::swap(&mut self.fiber_refs, &mut ctx.fiber_refs);
        mem::swap(&mut self.mask_depth, &mut ctx.mask_depth);
        mem::swap(&mut self.interrupt_requested, &mut ctx.interrupt_requested);
        mem::swap(&mut self.seeded_random, &mut ctx.seeded_random);
        mem::swap(
            &mut self.pending_resume_close,
            &mut ctx.pending_resume_close,
        );
        mem::swap(
            &mut self.pending_provide_result,
            &mut ctx.pending_provide_result,
        );
        mem::swap(&mut self.pending_layer_build, &mut ctx.pending_layer_build);
    }

    fn switch_to_fiber(&mut self, next_id: usize) {
        if next_id == self.active_fiber {
            self.quantum = FIBER_QUANTUM;
            return;
        }
        let mut next_ctx = self
            .parked_fibers
            .remove(&next_id)
            .expect("scheduled fiber context exists");
        let prev_id = self.active_fiber;
        self.swap_fiber_context(&mut next_ctx);
        self.parked_fibers.insert(prev_id, next_ctx);
        self.active_fiber = next_id;
        self.quantum = FIBER_QUANTUM;
    }

    fn snapshot_active_services(&mut self) -> Result<Vec<Value>, Error> {
        let mut reps = Vec::new();
        for frame in self.frames.iter().rev() {
            Self::collect_dest_services(&frame.destination, &self.providers, &mut reps);
        }
        for s in &self.inherited_services {
            reps.push(s.representation());
        }
        let mut owned = Vec::with_capacity(reps.len());
        for rep in reps {
            match self.heap.dup(&rep) {
                Ok(v) => owned.push(v),
                Err(err) => {
                    self.heap.drop_values(owned)?;
                    return Err(err);
                }
            }
        }
        Ok(owned)
    }

    fn collect_dest_services(
        dest: &Destination,
        providers: &BTreeMap<usize, Provider>,
        reps: &mut Vec<Value>,
    ) {
        match dest {
            Destination::LayerRun { provider, .. } => {
                if let Some(p) = providers.get(provider) {
                    reps.push(p.layer.representation());
                }
            }
            Destination::ProvideContext { context, .. } => {
                reps.push(context.representation());
            }
            Destination::WithServices { services, next } => {
                for s in services {
                    reps.push(s.representation());
                }
                Self::collect_dest_services(next, providers, reps);
            }
            Destination::RestoreMask { next, .. } | Destination::CachedComplete { next, .. } => {
                Self::collect_dest_services(next, providers, reps);
            }
            _ => {}
        }
    }

    fn snapshot_fiber_refs(&mut self) -> Result<BTreeMap<String, Value>, Error> {
        let mut out = BTreeMap::new();
        let pairs: Vec<(String, Value)> = self
            .fiber_refs
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect();
        for (k, rep) in pairs {
            match self.heap.dup(&rep) {
                Ok(v) => {
                    out.insert(k, v);
                }
                Err(err) => {
                    self.heap.drop_values(out.into_values().collect())?;
                    return Err(err);
                }
            }
        }
        Ok(out)
    }

    fn alloc_fiber_id(&mut self) -> usize {
        let id = self.next_fiber;
        self.next_fiber = self.next_fiber.checked_add(1).expect("fiber id overflow");
        id
    }

    fn runtime_metrics_enabled(&self, services: &[Value]) -> bool {
        let mut budget = 64u64;
        for s in services {
            if let Ok(Some(val)) = layers::lookup_layer_service(
                self.heap,
                s,
                "effect/Metric/RuntimeEnabled",
                &mut budget,
            ) {
                return matches!(val, Value::Bool(true));
            }
        }
        false
    }

    fn snapshot_active_tail_ops(&mut self) -> Result<Vec<InstalledOperation>, Error> {
        let mut specs: Vec<(String, String, usize, usize, Value)> = Vec::new();
        for frame in self.frames.iter().rev() {
            if let Destination::Handle { activation, .. } = frame.destination
                && let Some(act) = self.activations.get(&activation)
            {
                for op in &act.operations {
                    if op.class == OpClass::Tail {
                        specs.push((
                            op.effect.clone(),
                            op.name.clone(),
                            op.dicts,
                            op.arity,
                            op.function.representation(),
                        ));
                    }
                }
            }
        }
        for op in &self.inherited_tail_ops {
            specs.push((
                op.effect.clone(),
                op.name.clone(),
                op.dicts,
                op.arity,
                op.function.representation(),
            ));
        }
        let mut owned = Vec::with_capacity(specs.len());
        for (effect, name, dicts, arity, rep) in specs {
            match self.heap.dup(&rep) {
                Ok(function) => owned.push(InstalledOperation {
                    effect,
                    name,
                    class: OpClass::Tail,
                    dicts,
                    arity,
                    function,
                }),
                Err(err) => {
                    self.heap
                        .drop_values(owned.into_iter().map(|o| o.function).collect())?;
                    return Err(err);
                }
            }
        }
        Ok(owned)
    }

    fn spawn_raw_fiber(
        &mut self,
        owner: usize,
        evidence: Option<OwnerEvidence>,
        task: Value,
        services: Vec<Value>,
        fiber_refs: BTreeMap<String, Value>,
        mask_depth: usize,
    ) -> Result<usize, Error> {
        let tail_ops = match self.snapshot_active_tail_ops() {
            Ok(ops) => ops,
            Err(err) => {
                self.heap.drop_value(task)?;
                self.heap.drop_values(services)?;
                self.heap.drop_values(fiber_refs.into_values().collect())?;
                return Err(err);
            }
        };
        let track_metric = self.runtime_metrics_enabled(&services);
        let child_id = self.alloc_fiber_id();
        let mut ctx = FiberContext::empty(owner, evidence);
        ctx.inherited_services = services;
        ctx.inherited_tail_ops = tail_ops;
        ctx.fiber_refs = fiber_refs;
        ctx.mask_depth = mask_depth;
        ctx.seeded_random = self.seeded_random;
        let caller = self.active_fiber;
        self.parked_fibers.insert(child_id, ctx);
        self.fiber_table.insert(child_id, FiberRecord::root());
        self.switch_to_fiber(child_id);
        self.scratch.push(task);
        self.scratch.push(Value::Unit);
        let res = self.await_task_fresh(Destination::Top);
        self.switch_to_fiber(caller);
        if let Err(err) = res {
            self.drop_failed_spawn(child_id)?;
            return Err(err);
        }
        if track_metric {
            self.runtime_fibers_started = self.runtime_fibers_started.saturating_add(1);
        }
        self.run_queue.push_back(child_id);
        Ok(child_id)
    }

    fn spawn_applied_fiber(
        &mut self,
        owner: usize,
        evidence: Option<OwnerEvidence>,
        func: Value,
        arg: Value,
        services: Vec<Value>,
        mask_depth: usize,
    ) -> Result<usize, Error> {
        let child_id = self.alloc_fiber_id();
        let mut ctx = FiberContext::empty(owner, evidence);
        ctx.inherited_services = services;
        ctx.mask_depth = mask_depth;
        let caller = self.active_fiber;
        self.parked_fibers.insert(child_id, ctx);
        self.fiber_table.insert(child_id, FiberRecord::root());
        self.switch_to_fiber(child_id);
        self.scratch.extend([func, arg]);
        let res = self.apply_fresh(Destination::Top);
        self.switch_to_fiber(caller);
        if let Err(err) = res {
            self.drop_failed_spawn(child_id)?;
            return Err(err);
        }
        self.run_queue.push_back(child_id);
        Ok(child_id)
    }

    fn drop_failed_spawn(&mut self, child_id: usize) -> Result<(), Error> {
        let mut roots = Vec::new();
        if let Some(ctx) = self.parked_fibers.remove(&child_id) {
            self.drain_fiber_ctx_roots(ctx, &mut roots);
        }
        self.fiber_table.remove(&child_id);
        self.heap.drop_values(roots)
    }
}
