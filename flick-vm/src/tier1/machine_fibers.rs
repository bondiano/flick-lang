//! Cooperative single-threaded fiber contexts, spawn, park/yield, interrupt, and wake for tier 1.

use std::collections::{BTreeMap, VecDeque};
use std::mem;

use flick_core::types::OpClass;

use crate::stdlib::{self, FIBER_QUANTUM};
use crate::tier0::{CauseToken, Error, ObjectId, Value};

use super::machine::{End, Frame, Machine, OwnerEvidence, RegVal};
use super::machine_handlers::InstalledOperation;
use super::machine_layers::{LayerBuildTarget, PendingProvideResult, Provider};
use super::machine_unwind::Discard;

pub(super) struct FiberContext {
    pub(super) current_owner: usize,
    pub(super) current_evidence: Option<OwnerEvidence>,
    pub(super) frames: Vec<Frame>,
    pub(super) registers: Vec<Option<RegVal>>,
    pub(super) discard: Option<Discard>,
    pub(super) ready: Option<usize>,
    pub(super) scratch: Vec<Value>,
    pub(super) pending: Option<RegVal>,
    pub(super) failure: Option<CauseToken>,
    pub(super) failure_elected: bool,
    pub(super) pending_handler: Option<RegVal>,
    pub(super) top: Option<RegVal>,
    pub(super) arguments: VecDeque<Value>,
    pub(super) inherited_services: Vec<Value>,
    pub(super) inherited_tail_ops: Vec<InstalledOperation>,
    pub(super) fiber_refs: BTreeMap<String, Value>,
    pub(super) mask_depth: usize,
    pub(super) interrupt_requested: Option<u64>,
    pub(super) seeded_random: Option<usize>,
    pub(super) pending_resume_close: Option<usize>,
    pub(super) pending_provide_result: Option<PendingProvideResult>,
    pub(super) pending_layer_build: Option<(Value, usize, usize)>,
}

impl FiberContext {
    pub(super) fn empty(owner: usize, evidence: Option<OwnerEvidence>) -> Self {
        Self {
            current_owner: owner,
            current_evidence: evidence,
            frames: Vec::new(),
            registers: Vec::new(),
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

pub(super) enum FiberOutcome {
    Running,
    Success(Value),
    Failure(CauseToken),
}

#[derive(Clone, Copy)]
pub(super) enum FiberWaitKind {
    Join(usize),
    Await(usize),
    Interrupt(usize),
    CoordDone { coord_id: usize, index: usize },
}

#[derive(Clone, Copy)]
pub(super) struct FiberWaiter {
    pub(super) fiber: usize,
    pub(super) kind: FiberWaitKind,
}

pub(super) struct SupervisorHook {
    pub(super) owner: usize,
    pub(super) callback: Value,
    pub(super) services: Vec<Value>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ParkReason {
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
}

pub(super) struct FiberRecord {
    pub(super) owner: Option<usize>,
    pub(super) outcome: FiberOutcome,
    pub(super) parked: Option<ParkReason>,
    pub(super) waiters: Vec<FiberWaiter>,
    pub(super) supervisor: Option<SupervisorHook>,
    pub(super) coord_parent: Option<(usize, usize)>,
    pub(super) group_watchers: Vec<(usize, usize)>,
    pub(super) is_supervisor_of: Option<usize>,
}

impl FiberRecord {
    pub(super) fn root() -> Self {
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

pub(super) enum TimerAction {
    WakeSleep { fiber: usize, dst: usize },
    FireTimeout { coord_id: usize },
}

pub(super) struct TimerEntry {
    pub(super) id: usize,
    pub(super) deadline_ns: u64,
    pub(super) action: TimerAction,
}

pub(super) struct TestClockAdvance {
    pub(super) caller_fiber: usize,
    pub(super) dst: usize,
    pub(super) target_ns: u64,
}

impl Machine<'_, '_> {
    pub(super) fn swap_fiber_context(&mut self, ctx: &mut FiberContext) {
        mem::swap(&mut self.current_owner, &mut ctx.current_owner);
        mem::swap(&mut self.current_evidence, &mut ctx.current_evidence);
        mem::swap(&mut self.frames, &mut ctx.frames);
        mem::swap(&mut self.registers, &mut ctx.registers);
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
        self.bump_handler_epoch();
    }

    pub(super) fn switch_to_fiber(&mut self, next_id: usize) {
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

    pub(super) fn snapshot_active_services(&mut self) -> Result<Vec<Value>, Error> {
        let mut reps = Vec::new();
        for frame in self.frames.iter().rev() {
            Self::collect_end_services(&frame.end, &self.providers, &mut reps);
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

    fn collect_end_services(
        end: &End,
        providers: &BTreeMap<usize, Provider>,
        reps: &mut Vec<Value>,
    ) {
        match end {
            End::LayerRun { provider, .. } => {
                if let Some(p) = providers.get(provider) {
                    reps.push(p.layer.representation());
                }
            }
            End::ProvideContext { context, .. } => {
                reps.push(context.representation());
            }
            End::WithServices { services, next } => {
                for s in services {
                    reps.push(s.representation());
                }
                Self::collect_end_services(next, providers, reps);
            }
            End::RestoreMask { next, .. } | End::CachedComplete { next, .. } => {
                Self::collect_end_services(next, providers, reps);
            }
            _ => {}
        }
    }

    pub(super) fn snapshot_fiber_refs(&mut self) -> Result<BTreeMap<String, Value>, Error> {
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

    pub(super) fn alloc_fiber_id(&mut self) -> usize {
        let id = self.next_fiber;
        self.next_fiber = self.next_fiber.checked_add(1).expect("fiber id overflow");
        id
    }

    fn runtime_metrics_enabled(&self, services: &[Value]) -> bool {
        let mut budget = 64u64;
        for s in services {
            if let Ok(Some(val)) = stdlib::runtime::layers::lookup_layer_service(
                self.heap,
                s,
                "effect/Metric/RuntimeEnabled",
                &mut stdlib::FuelCounter(&mut budget),
            ) {
                return matches!(val, Value::Bool(true));
            }
        }
        false
    }

    fn snapshot_active_tail_ops(&mut self) -> Result<Vec<InstalledOperation>, Error> {
        enum OpFnSpec {
            Static(usize),
            Heap(Value),
        }
        let mut specs: Vec<(usize, usize, usize, OpFnSpec)> = Vec::new();
        for frame in self.frames.iter().rev() {
            if let End::Handle { activation, .. } = frame.end
                && let Some(act) = self.activations.get(&activation)
            {
                for op in &act.operations {
                    if op.class == OpClass::Tail {
                        let f = match &op.function {
                            RegVal::StaticClosure(c) => OpFnSpec::Static(*c),
                            RegVal::Value(v) => OpFnSpec::Heap(v.representation()),
                        };
                        specs.push((op.op_id, op.dicts, op.arity, f));
                    }
                }
            }
        }
        for op in &self.inherited_tail_ops {
            let f = match &op.function {
                RegVal::StaticClosure(c) => OpFnSpec::Static(*c),
                RegVal::Value(v) => OpFnSpec::Heap(v.representation()),
            };
            specs.push((op.op_id, op.dicts, op.arity, f));
        }
        let mut owned: Vec<InstalledOperation> = Vec::with_capacity(specs.len());
        for (op_id, dicts, arity, spec) in specs {
            let function = match spec {
                OpFnSpec::Static(c) => RegVal::StaticClosure(c),
                OpFnSpec::Heap(rep) => match self.heap.dup(&rep) {
                    Ok(v) => RegVal::Value(v),
                    Err(err) => {
                        let mut roots = Vec::new();
                        for o in owned {
                            Self::reg_into_roots(o.function, &mut roots);
                        }
                        self.heap.drop_values(roots)?;
                        return Err(err);
                    }
                },
            };
            owned.push(InstalledOperation {
                op_id,
                class: OpClass::Tail,
                dicts,
                arity,
                function,
            });
        }
        Ok(owned)
    }

    pub(super) fn spawn_raw_fiber(
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
        let res = self.await_task_reg(RegVal::Value(task), End::Top);
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

    pub(super) fn spawn_applied_fiber(
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
        let res = self.apply_reg(RegVal::Value(func), RegVal::Value(arg), End::Top, false);
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
