//! Tier-1 register bytecode virtual machine core, main loop, and leak-free cleanup.

use std::collections::{BTreeMap, VecDeque};

use flick_core::ir::memory::MemorySpan;

use crate::stdlib::primitives::IsaacRng;
use crate::stdlib::{DEFAULT_VIRTUAL_NOW_NS, FIBER_QUANTUM, HostServices};
use crate::tier0::causes::StepFailure;
use crate::tier0::{
    CauseToken, Data, Error, ExecutionReport, Heap, HostMode, LanguageExit, ObjectId,
    ProgramOutcome, Value, program as tier0_program,
};

use super::machine_coords::{CachedCellState, CoordState, DeferredState};
use super::machine_fibers::{
    FiberContext, FiberOutcome, FiberRecord, TestClockAdvance, TimerEntry,
};
use super::machine_generators::GeneratorSlot;
use super::machine_groups::FiberGroupState;
use super::machine_handlers::{
    Activation, CachedHandlerHit, CapturedSlot, InstalledOperation, Invocation, Segment,
};
use super::machine_layers::{LayerBuildTarget, PendingProvide, PendingProvideResult, Provider};
use super::machine_layers_build::MemoMapState;
use super::machine_scopes::{ClosingScopeState, OwnerState};
pub(super) use super::machine_state::{
    End, Frame, GlobalState, LoopState, MainStage, OwnerEvidence, RecoveryState, RegVal,
};
use super::machine_unwind::Discard;
use super::opcode::{Instr, Prepared};

pub(super) struct Machine<'p, 'h> {
    pub(super) heap: &'h mut Heap,
    pub(super) image: &'p Prepared,
    pub(super) execution: usize,
    pub(super) frames: Vec<Frame>,
    pub(super) registers: Vec<Option<RegVal>>,
    pub(super) globals: Vec<GlobalState>,
    pub(super) cells: Vec<Value>,
    pub(super) lazies: Vec<Value>,
    pub(super) owners: Vec<OwnerState>,
    pub(super) current_owner: usize,
    pub(super) current_evidence: Option<OwnerEvidence>,
    pub(super) mask_depth: usize,
    pub(super) activations: BTreeMap<usize, Activation>,
    pub(super) providers: BTreeMap<usize, Provider>,
    pub(super) generators: BTreeMap<usize, GeneratorSlot>,
    pub(super) root: Option<usize>,
    pub(super) segments: BTreeMap<usize, Segment>,
    pub(super) continuations: BTreeMap<usize, CapturedSlot>,
    pub(super) invocations: BTreeMap<usize, Invocation>,
    pub(super) discard: Option<Discard>,
    pub(super) ready: Option<usize>,
    pub(super) next_handler: usize,
    pub(super) next_handler_epoch: u64,
    pub(super) handler_epoch: u64,
    pub(super) handler_cache_epoch: u64,
    pub(super) handler_cache: BTreeMap<usize, Option<CachedHandlerHit>>,
    #[cfg(test)]
    pub(super) handler_cache_hits: u64,
    #[cfg(test)]
    pub(super) handler_cache_misses: u64,
    pub(super) last_defect_span: Option<MemorySpan>,
    pub(super) scratch: Vec<Value>,
    pub(super) pending: Option<RegVal>,
    pub(super) failure: Option<CauseToken>,
    pub(super) failure_elected: bool,
    pub(super) recoveries: BTreeMap<usize, RecoveryState>,
    pub(super) next_recovery: usize,
    pub(super) pending_handler: Option<RegVal>,
    pub(super) arguments: VecDeque<Value>,
    pub(super) top: Option<RegVal>,
    pub(super) main_stage: MainStage,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) program_args: Vec<String>,
    pub(super) host_services: HostServices,
    pub(super) fuel: u64,
    pub(super) host_mode: HostMode,
    // Cooperative single-threaded scheduler & async state.
    pub(super) active_fiber: usize,
    pub(super) next_fiber: usize,
    pub(super) quantum: u32,
    pub(super) parked_fibers: BTreeMap<usize, FiberContext>,
    pub(super) fiber_table: BTreeMap<usize, FiberRecord>,
    pub(super) run_queue: VecDeque<usize>,
    pub(super) virtual_now_ns: u64,
    pub(super) test_clock_mode: bool,
    pub(super) test_clock_waiter: Option<TestClockAdvance>,
    pub(super) timers: BTreeMap<(u64, usize), TimerEntry>,
    pub(super) next_timer: usize,
    pub(super) deferreds: BTreeMap<usize, DeferredState>,
    pub(super) next_deferred: usize,
    pub(super) coords: BTreeMap<usize, CoordState>,
    pub(super) next_coord: usize,
    pub(super) fiber_groups: BTreeMap<usize, FiberGroupState>,
    pub(super) next_group: usize,
    pub(super) memo_maps: BTreeMap<usize, MemoMapState>,
    pub(super) next_memo: usize,
    pub(super) cached_cells: BTreeMap<usize, CachedCellState>,
    pub(super) next_cached: usize,
    pub(super) isaac_rngs: BTreeMap<usize, IsaacRng>,
    pub(super) next_rng: usize,
    pub(super) closing_scopes: BTreeMap<usize, Box<ClosingScopeState>>,
    pub(super) pending_provides: BTreeMap<usize, PendingProvide>,
    pub(super) next_provide: usize,
    pub(super) pending_resume_close: Option<usize>,
    pub(super) pending_provide_result: Option<PendingProvideResult>,
    pub(super) pending_layer_build: Option<(Value, usize, usize)>,
    pub(super) inherited_services: Vec<Value>,
    pub(super) inherited_tail_ops: Vec<InstalledOperation>,
    pub(super) lazy_waiters: BTreeMap<ObjectId, Vec<(usize, usize)>>,
    pub(super) fiber_refs: BTreeMap<String, Value>,
    pub(super) interrupt_requested: Option<u64>,
    pub(super) seeded_random: Option<usize>,
    pub(super) root_defects: Vec<CauseToken>,
    pub(super) runtime_fibers_started: u64,
}

pub(super) fn execute_prepared(
    heap: &mut Heap,
    image: &Prepared,
    arguments: Vec<Value>,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
    main_protocol: bool,
) -> ExecutionReport {
    if let Err(error) = heap.ensure_owned(&arguments) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(error),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    if let Err(error) = heap.admit_values(&arguments) {
        let _ = heap.drop_values(arguments);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(error),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let mut machine = Machine::new(
        heap,
        image,
        arguments,
        fuel,
        host_mode,
        program_args,
        main_protocol,
    );
    let result = machine.start_entry().and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

#[cfg(test)]
pub(super) fn execute_prepared_cache_stats(
    heap: &mut Heap,
    image: &Prepared,
    fuel: u64,
) -> (ExecutionReport, u64, u64, u64) {
    let mut machine = Machine::new(
        heap,
        image,
        Vec::new(),
        fuel,
        HostMode::Deterministic,
        Vec::new(),
        false,
    );
    let result = machine.start_entry().and_then(|()| machine.run());
    let outcome = machine.finish(result);
    let report = ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    };
    (
        report,
        machine.handler_cache_hits,
        machine.handler_cache_misses,
        machine.handler_epoch,
    )
}

impl<'p, 'h> Machine<'p, 'h> {
    fn new(
        heap: &'h mut Heap,
        image: &'p Prepared,
        arguments: Vec<Value>,
        fuel: u64,
        host_mode: HostMode,
        program_args: Vec<String>,
        main_protocol: bool,
    ) -> Self {
        let main_stage = if !main_protocol {
            MainStage::Done
        } else if image.root.is_some() {
            MainStage::Root { main: image.entry }
        } else {
            MainStage::Function
        };
        let mut fiber_table = BTreeMap::new();
        fiber_table.insert(1, FiberRecord::root());
        Self {
            heap,
            image,
            execution: tier0_program::next_execution_id(),
            frames: Vec::new(),
            registers: Vec::new(),
            globals: (0..image.globals.len())
                .map(|_| GlobalState::Empty)
                .collect(),
            cells: Vec::new(),
            lazies: Vec::new(),
            owners: vec![OwnerState::default()],
            current_owner: 0,
            current_evidence: None,
            mask_depth: 0,
            activations: BTreeMap::new(),
            providers: BTreeMap::new(),
            generators: BTreeMap::new(),
            root: None,
            segments: BTreeMap::new(),
            continuations: BTreeMap::new(),
            invocations: BTreeMap::new(),
            discard: None,
            ready: None,
            next_handler: 0,
            next_handler_epoch: 1,
            handler_epoch: 0,
            handler_cache_epoch: 0,
            handler_cache: BTreeMap::new(),
            #[cfg(test)]
            handler_cache_hits: 0,
            #[cfg(test)]
            handler_cache_misses: 0,
            last_defect_span: None,
            scratch: Vec::new(),
            pending: None,
            failure: None,
            failure_elected: false,
            recoveries: BTreeMap::new(),
            next_recovery: 0,
            pending_handler: None,
            arguments: arguments.into(),
            top: None,
            main_stage,
            stdout: String::new(),
            stderr: String::new(),
            program_args,
            host_services: HostServices::new(),
            fuel,
            host_mode,
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
            next_timer: 1,
            deferreds: BTreeMap::new(),
            next_deferred: 1,
            coords: BTreeMap::new(),
            next_coord: 1,
            fiber_groups: BTreeMap::new(),
            next_group: 1,
            memo_maps: BTreeMap::new(),
            next_memo: 1,
            cached_cells: BTreeMap::new(),
            next_cached: 1,
            isaac_rngs: BTreeMap::new(),
            next_rng: 1,
            closing_scopes: BTreeMap::new(),
            pending_provides: BTreeMap::new(),
            next_provide: 1,
            pending_resume_close: None,
            pending_provide_result: None,
            pending_layer_build: None,
            inherited_services: Vec::new(),
            inherited_tail_ops: Vec::new(),
            lazy_waiters: BTreeMap::new(),
            fiber_refs: BTreeMap::new(),
            interrupt_requested: None,
            seeded_random: None,
            root_defects: Vec::new(),
            runtime_fibers_started: 0,
        }
    }
}

impl Machine<'_, '_> {
    fn start_entry(&mut self) -> Result<(), Error> {
        let start = if matches!(self.main_stage, MainStage::Root { .. }) {
            self.image.root.expect("rooted image has root global")
        } else {
            self.image.entry
        };
        self.start_global_top(start)
    }

    fn start_global_top(&mut self, id: usize) -> Result<(), Error> {
        match &self.globals[id] {
            GlobalState::Ready(reg) => {
                let dup = match reg {
                    RegVal::StaticClosure(code) => RegVal::StaticClosure(*code),
                    RegVal::Value(val) => {
                        self.heap.admit_ordinary(val)?;
                        let rep = val.representation();
                        RegVal::Value(self.heap.dup(&rep)?)
                    }
                };
                self.top = Some(dup);
                Ok(())
            }
            GlobalState::Initializing => {
                Err(Error::GlobalCycle(self.image.global_names[id].clone()))
            }
            GlobalState::Empty => {
                self.globals[id] = GlobalState::Initializing;
                self.push_call(
                    self.image.globals[id],
                    [],
                    End::Global {
                        id,
                        dst: usize::MAX,
                    },
                )
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run(&mut self) -> Result<LanguageExit, Error> {
        let image = self.image;
        loop {
            if let Some(owner) = self.pending_resume_close.take() {
                self.resume_owner_close(owner)?;
                continue;
            }
            if let Some(res) = self.pending_provide_result.take() {
                match res {
                    PendingProvideResult::Ready(pid, ctx) => {
                        self.complete_dynamic_provide_ok(pid, ctx)?;
                    }
                    PendingProvideResult::Failed(pid, cause) => {
                        self.complete_dynamic_provide_err(pid, cause)?;
                    }
                }
                continue;
            }
            if let Some((layer, memo_id, sub_owner)) = self.pending_layer_build.take() {
                self.begin_build_layer(layer, memo_id, sub_owner, false, LayerBuildTarget::Top)?;
                continue;
            }
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            self.drain_dropped_continuations();
            if self.discard.is_none()
                && self.ready.is_none()
                && self.failure.is_some()
                && self.frames.is_empty()
            {
                let cause = self.failure.take().expect("failure root");
                if self.active_fiber != 1 {
                    self.complete_child_fiber(FiberOutcome::Failure(cause))?;
                    continue;
                }
                let final_cause = self
                    .combine_root_defects(Some(cause))?
                    .expect("root failure");
                return Ok(LanguageExit::Failure(final_cause));
            }
            if self.discard.is_none()
                && self.ready.is_none()
                && self.frames.is_empty()
                && self.arguments.is_empty()
                && self.top.is_some()
            {
                if self.active_fiber != 1 {
                    let reg = self.top.take().expect("child fiber return");
                    let val = self.materialize_reg(reg);
                    self.complete_child_fiber(FiberOutcome::Success(val))?;
                    continue;
                }
                if self.main_stage != MainStage::Done {
                    self.advance_main()?;
                    continue;
                }
                let reg = self.top.take().expect("completed top value");
                let value = self.materialize_reg(reg);
                if !self.root_defects.is_empty() {
                    self.heap.drop_value(value)?;
                    let cause = self.combine_root_defects(None)?.expect("root defects");
                    return Ok(LanguageExit::Failure(cause));
                }
                return Ok(LanguageExit::Success(value));
            }
            if self.discard.is_none()
                && self.ready.is_none()
                && self.failure.is_none()
                && self.frames.is_empty()
                && self.top.is_none()
            {
                return Err(Error::UnsupportedOperation(
                    "deadlock: no runnable fibers remain",
                ));
            }
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            self.maybe_quantum_preempt()?;
            if self.pending_resume_close.is_some()
                || self.pending_provide_result.is_some()
                || self.pending_layer_build.is_some()
            {
                continue;
            }
            if self.discard.is_some() {
                self.discard_step()?;
            } else if let Some(id) = self.ready.take() {
                self.invoke_step(id)?;
            } else if self.failure.is_some() {
                self.unwind_frame()?;
            } else if let Some(callee) = self.top.take() {
                let arg = self
                    .arguments
                    .pop_front()
                    .expect("top application has an argument");
                self.apply_reg(callee, RegVal::Value(arg), End::Top, false)?;
            } else if !self.frames.is_empty() {
                let (code, pc) = {
                    let frame = self.frames.last_mut().expect("running frame");
                    let at = frame.pc;
                    frame.pc += 1;
                    (frame.code, at)
                };
                let instr = &image.functions[code].instructions[pc];
                if let Err(error) = self.step_at(code, pc, instr) {
                    match StepFailure::classify(error) {
                        StepFailure::Language(spec) => {
                            let span = image
                                .span(code, pc)
                                .or_else(|| self.last_defect_span.take());
                            self.last_defect_span = None;
                            self.raise_defect(spec.with_span(span))?;
                        }
                        StepFailure::Evaluator(err) => return Err(err),
                    }
                }
            }
        }
    }

    fn advance_main(&mut self) -> Result<(), Error> {
        if let MainStage::Root { main } = self.main_stage {
            self.install_root()?;
            self.main_stage = MainStage::Function;
            return self.start_global_top(main);
        }
        let stage = self.main_stage;
        self.main_stage = if stage == MainStage::Function {
            MainStage::Task
        } else {
            MainStage::Done
        };
        let callable = match (self.top.as_ref(), stage) {
            (Some(RegVal::StaticClosure(_)), MainStage::Function) => true,
            (Some(RegVal::Value(Value::Object(tok))), MainStage::Function) => {
                matches!(self.heap.object(tok.id)?.data, Data::Closure { .. })
            }
            (Some(RegVal::Value(Value::Object(tok))), MainStage::Task) => matches!(
                self.heap.object(tok.id)?.data,
                Data::Task { .. }
                    | Data::MaskedTask { .. }
                    | Data::CachedRunTask { .. }
                    | Data::CachedInvalidateTask { .. }
                    | Data::ExitTask(_)
            ),
            _ => false,
        };
        if callable {
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            let callee = self.top.take().expect("main callable");
            if stage == MainStage::Function {
                self.apply_reg(callee, RegVal::Value(Value::Unit), End::Top, false)?;
            } else {
                self.await_task_reg(callee, End::Top)?;
            }
        }
        Ok(())
    }

    pub(super) fn restore_evidence(&mut self) {
        self.current_evidence = self.frames.last().and_then(|frame| frame.evidence);
    }

    fn step_at(&mut self, code: usize, pc: usize, instr: &Instr) -> Result<(), Error> {
        if let Instr::Perform {
            dst,
            op_id,
            dicts,
            args,
        } = instr
        {
            let span = self.image.span(code, pc);
            return self.exec_perform(*dst, *op_id, dicts, args, span);
        }
        self.step_instr(instr)
    }
}
