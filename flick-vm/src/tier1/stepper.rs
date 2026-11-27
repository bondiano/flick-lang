//! Steppable tier-1 execution for safe-point hot swaps and migrations.
//!
//! A [`SuspendedTier1`] owns its compiled [`Prepared`] image and live frame
//! stack without borrowing the session's [`Heap`] or [`SessionPersistentState`]
//! between steps. Hot swaps extend its [`Prepared`] image monotonically via
//! [`super::session::extend_prepared`] so frames already on the stack finish on
//! old code while subsequent `Instr::Global` reads observe swapped definitions.

use std::collections::BTreeSet;
use std::mem;

use flick_core::ir::memory::{CodeId, Name, Program};

use crate::tier0::causes::StepFailure;
use crate::tier0::{
    Error, ExecutionReport, Heap, HostMode, LanguageExit, ProgramOutcome, SessionPersistentState,
};

use super::machine::{End, GlobalState, Machine, MainStage, RegVal};
use super::machine_fibers::FiberOutcome;
use super::machine_generators::GeneratorRun;
use super::machine_layers::{LayerBuildTarget, PendingProvideResult};
use super::opcode::{PrepareError, Prepared};
use super::session::{extend_prepared, load_persistent_into_machine, save_persistent_from_machine};
use super::stepper_state::Tier1State;

/// Result of advancing a [`SuspendedTier1`] execution by up to `max_steps`.
#[derive(Debug)]
pub(crate) enum Tier1StepResult {
    Running { steps_taken: usize },
    Finished(ExecutionReport),
}

enum StepLoopExit {
    Paused(usize),
    Finished(LanguageExit),
}

/// A paused tier-1 VM execution that survives across safe-point hot swaps and
/// state migrations without holding borrows into `VmSession`.
pub(crate) struct SuspendedTier1 {
    image: Prepared,
    execution: usize,
    state: Tier1State,
}

impl SuspendedTier1 {
    /// Starts a new steppable tier-1 computation rooted at `entry`.
    pub(crate) fn start(
        heap: &mut Heap,
        image: Prepared,
        execution: usize,
        persistent: &mut SessionPersistentState,
        entry: &Name,
        fuel: u64,
        host_mode: HostMode,
    ) -> Result<Self, Error> {
        let entry_id = image
            .global_names
            .iter()
            .position(|n| n == entry)
            .ok_or_else(|| Error::UnknownGlobal(entry.clone()))?;
        let state = {
            let mut machine =
                Machine::new(heap, &image, Vec::new(), fuel, host_mode, Vec::new(), false);
            machine.execution = execution;
            load_persistent_into_machine(&mut machine, persistent);
            if let Err(err) = machine.start_global_top(entry_id) {
                save_persistent_from_machine(&mut machine, persistent);
                let _ = machine.cleanup();
                return Err(err);
            }
            save_persistent_from_machine(&mut machine, persistent);
            Tier1State::from_machine(&mut machine)
        };
        Ok(Self {
            image,
            execution,
            state,
        })
    }

    /// Advances this computation by up to `max_steps` bytecode steps.
    pub(crate) fn step_n(
        &mut self,
        heap: &mut Heap,
        persistent: &mut SessionPersistentState,
        max_steps: usize,
    ) -> Tier1StepResult {
        let image = &self.image;
        let state = &mut self.state;
        let mut machine = Machine::new(
            heap,
            image,
            Vec::new(),
            state.fuel,
            state.host_mode,
            Vec::new(),
            false,
        );
        machine.execution = self.execution;
        state.restore(&mut machine);
        load_persistent_into_machine(&mut machine, persistent);
        let res = machine.run_steps(max_steps);
        match res {
            Ok(StepLoopExit::Paused(steps_taken)) => {
                save_persistent_from_machine(&mut machine, persistent);
                state.capture(&mut machine);
                Tier1StepResult::Running { steps_taken }
            }
            Ok(StepLoopExit::Finished(exit)) => {
                let outcome = match machine.report_exit(exit) {
                    Ok(o) => o,
                    Err(e) => ProgramOutcome::Fault(e),
                };
                save_persistent_from_machine(&mut machine, persistent);
                let outcome = if let Err(err) = machine.cleanup() {
                    let _ = outcome.release(machine.heap);
                    ProgramOutcome::Fault(err)
                } else {
                    outcome
                };
                Tier1StepResult::Finished(ExecutionReport {
                    outcome,
                    stdout: mem::take(&mut machine.stdout),
                    stderr: mem::take(&mut machine.stderr),
                })
            }
            Err(error) => {
                save_persistent_from_machine(&mut machine, persistent);
                let _ = machine.cleanup();
                Tier1StepResult::Finished(ExecutionReport {
                    outcome: ProgramOutcome::Fault(error),
                    stdout: mem::take(&mut machine.stdout),
                    stderr: mem::take(&mut machine.stderr),
                })
            }
        }
    }

    /// Prepares a candidate extended [`Prepared`] image for a hot swap without
    /// mutating this task yet, so commit stays atomic.
    pub(crate) fn stage_hot_swap(
        &self,
        program: &Program,
        swapped: &[Name],
        extra_codes: &BTreeSet<CodeId>,
    ) -> Result<Prepared, PrepareError> {
        let mut candidate = self.image.clone();
        extend_prepared(&mut candidate, program, swapped, extra_codes)?;
        Ok(candidate)
    }

    /// Commits a pre-verified extended [`Prepared`] image onto this paused task.
    pub(crate) fn commit_hot_swap(&mut self, extended: Prepared) {
        self.image = extended;
    }

    /// Returns all IR `CodeId`s currently pinned by suspended frames,
    /// continuations, or generators so retirement never reclaims active code.
    pub(crate) fn pinned_codes(&self) -> BTreeSet<CodeId> {
        let mut out = BTreeSet::new();
        for frame in &self.state.frames {
            if let Some(&ir) = self.image.ir_codes.get(frame.code) {
                out.insert(ir);
            }
        }
        for seg in self.state.segments.values() {
            for frame in &seg.frames.frames {
                if let Some(&ir) = self.image.ir_codes.get(frame.code) {
                    out.insert(ir);
                }
            }
        }
        for slot in self.state.continuations.values() {
            if let Some(captured) = &slot.continuation {
                for frame in &captured.frames.frames {
                    if let Some(&ir) = self.image.ir_codes.get(frame.code) {
                        out.insert(ir);
                    }
                }
            }
        }
        for gen_slot in self.state.generators.values() {
            if let GeneratorRun::Suspended { frames, .. } = &gen_slot.run {
                for frame in &frames.frames {
                    if let Some(&ir) = self.image.ir_codes.get(frame.code) {
                        out.insert(ir);
                    }
                }
            }
        }
        out
    }

    /// Aborts this suspended computation and releases all owned heap roots.
    pub(crate) fn abort(mut self, heap: &mut Heap) -> Result<(), Error> {
        let image = &self.image;
        let state = &mut self.state;
        let mut machine = Machine::new(
            heap,
            image,
            Vec::new(),
            state.fuel,
            state.host_mode,
            Vec::new(),
            false,
        );
        machine.execution = self.execution;
        state.restore(&mut machine);
        machine.globals.fill_with(|| GlobalState::Empty);
        machine.cleanup()
    }
}

impl Machine<'_, '_> {
    #[allow(clippy::too_many_lines)]
    fn run_steps(&mut self, max_steps: usize) -> Result<StepLoopExit, Error> {
        let image = self.image;
        let mut steps_taken = 0usize;
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
                return Ok(StepLoopExit::Finished(LanguageExit::Failure(final_cause)));
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
                    return Ok(StepLoopExit::Finished(LanguageExit::Failure(cause)));
                }
                return Ok(StepLoopExit::Finished(LanguageExit::Success(value)));
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
            if steps_taken >= max_steps {
                return Ok(StepLoopExit::Paused(steps_taken));
            }
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            steps_taken += 1;
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
}
