//! Steppable two-version execution and tier-1 cached evaluation on [`super::VmSession`].

use std::collections::BTreeSet;
use std::mem;
use std::slice;
use std::sync::Arc;

use flick_core::ir::memory::{CodeId, Name, Program};

use crate::sandbox::{SandboxError, check_program_capabilities};
use crate::tier0::{
    Error, Heap, ProgramOutcome, SessionRunSpec, SuspendedTier0, Tier0StepResult, abort_tier0_step,
    execute_session_step, start_tier0_step, step_tier0_n,
};
use crate::tier1::PreparedStats;
use crate::tier1::opcode::Prepared;
use crate::tier1::session as tier1_session;
use crate::tier1::stepper::{SuspendedTier1, Tier1StepResult};

use super::hotswap::prepare_verified_tier1;
use super::hotswap_types::{ExecutionTier, HotSwapError, StepEvalOutcome, StepTaskId};
use super::program::SessionProgram;
use super::{EvalOutcome, VmSession};

/// Cached verified tier-1 bytecode image together with all global bindings it
/// depends on for invalidation.
pub(crate) struct CachedPrepared {
    pub(crate) image: Arc<Prepared>,
    pub(crate) deps: BTreeSet<Name>,
}

/// Active suspended evaluation owned by [`VmSession`].
pub(crate) enum SuspendedTask {
    Tier0(Box<SuspendedTier0>),
    Tier1(Box<SuspendedTier1>),
}

impl SuspendedTask {
    pub(crate) fn pinned_codes(&self) -> BTreeSet<CodeId> {
        match self {
            Self::Tier0(t0) => t0.pinned_codes(),
            Self::Tier1(t1) => t1.pinned_codes(),
        }
    }
}

impl VmSession {
    /// Returns the number of currently cached verified tier-1 bytecode images.
    #[must_use]
    pub fn tier1_cache_len(&self) -> usize {
        self.tier1_cache.len()
    }

    /// Evaluates an already-loaded global `entry` on `tier` (reusing verified
    /// cached bytecode on [`ExecutionTier::Tier1`], re-preparing after any swap
    /// invalidation).
    pub fn eval_existing_on_tier(&mut self, tier: ExecutionTier, entry: &Name) -> EvalOutcome {
        match tier {
            ExecutionTier::Tier0 => {
                let spec = SessionRunSpec {
                    entry,
                    await_task: false,
                    root_layer: self.root_layer.as_ref(),
                    provided_layers: &[],
                    handlers: &[],
                    fuel: self.fuel,
                    host_mode: self.host_mode,
                    program_args: &self.program_args,
                    external_interrupt: None,
                    synthetic_codes: SessionProgram::synthetic_codes(),
                    sandbox: &self.sandbox,
                };
                let report = execute_session_step(
                    &mut self.heap,
                    self.program.program(),
                    self.execution,
                    &mut self.persistent,
                    &spec,
                    self.host_callbacks.as_deref_mut(),
                );
                self.classify_value_report(report)
            }
            ExecutionTier::Tier1 => {
                let image = match self.get_or_prepare_tier1(entry, None) {
                    Ok(img) => img,
                    Err(err) => {
                        return EvalOutcome::Fault {
                            message: err.to_string(),
                            stdout: String::new(),
                        };
                    }
                };
                let report = tier1_session::execute_session_prepared(
                    &mut self.heap,
                    &image,
                    self.execution,
                    &mut self.persistent,
                    entry,
                    Vec::new(),
                    self.fuel,
                    self.host_mode,
                );
                self.classify_value_report(report)
            }
        }
    }

    /// Loads `incoming` and evaluates `entry` on `tier`.
    pub fn eval_phrase_on_tier(
        &mut self,
        tier: ExecutionTier,
        incoming: &Program,
        entry: &Name,
    ) -> EvalOutcome {
        if let Err(err) = check_program_capabilities(
            incoming,
            Some(slice::from_ref(entry)),
            self.sandbox.capabilities,
        ) {
            return match err {
                SandboxError::CapabilityDenied {
                    capability,
                    binding,
                    span,
                } => EvalOutcome::CapabilityDenied {
                    capability,
                    binding,
                    span,
                    stdout: String::new(),
                },
                other => EvalOutcome::Fault {
                    message: other.to_string(),
                    stdout: String::new(),
                },
            };
        }
        self.program.load(incoming);
        let outcome = self.eval_existing_on_tier(tier, entry);
        if matches!(
            outcome,
            EvalOutcome::TypedError { .. }
                | EvalOutcome::UnhandledEffect { .. }
                | EvalOutcome::Interrupted { .. }
                | EvalOutcome::CapabilityDenied { .. }
                | EvalOutcome::LimitExceeded { .. }
                | EvalOutcome::Fault { .. }
        ) {
            self.rollback_module(&entry.module);
        }
        outcome
    }

    /// Loads `incoming` and starts a steppable evaluation of `entry` on the
    /// session's active tier (`self.tier()`).
    pub fn start_step_eval(
        &mut self,
        incoming: &Program,
        entry: &Name,
    ) -> Result<StepTaskId, HotSwapError> {
        self.start_step_eval_on_tier(incoming, entry, self.tier)
    }

    /// Loads `incoming` and starts a steppable evaluation of `entry` on `tier`.
    pub fn start_step_eval_on_tier(
        &mut self,
        incoming: &Program,
        entry: &Name,
        tier: ExecutionTier,
    ) -> Result<StepTaskId, HotSwapError> {
        if let Err(err) = check_program_capabilities(
            incoming,
            Some(slice::from_ref(entry)),
            self.sandbox.capabilities,
        ) {
            return Err(match err {
                SandboxError::CapabilityDenied {
                    capability,
                    binding,
                    span,
                } => HotSwapError::CapabilityDenied {
                    capability,
                    binding,
                    span,
                },
                other => HotSwapError::InvalidIr(other.to_string()),
            });
        }
        self.program.load(incoming);
        self.start_step_existing(entry, tier)
    }

    /// Starts a steppable evaluation of an already-loaded global `entry` on `tier`.
    pub fn start_step_existing(
        &mut self,
        entry: &Name,
        tier: ExecutionTier,
    ) -> Result<StepTaskId, HotSwapError> {
        let task = match tier {
            ExecutionTier::Tier0 => {
                let t0 = start_tier0_step(
                    &mut self.heap,
                    self.program.program(),
                    self.execution,
                    &mut self.persistent,
                    entry,
                    self.fuel,
                    self.host_mode,
                    &self.sandbox,
                )
                .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                SuspendedTask::Tier0(Box::new(t0))
            }
            ExecutionTier::Tier1 => {
                let extra_codes = self.all_pinned_codes();
                let prepared =
                    prepare_verified_tier1(self.program.program(), entry, None, &extra_codes)?;
                let t1 = SuspendedTier1::start(
                    &mut self.heap,
                    prepared,
                    self.execution,
                    &mut self.persistent,
                    entry,
                    self.fuel,
                    self.host_mode,
                )
                .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                SuspendedTask::Tier1(Box::new(t1))
            }
        };
        let id = self.next_step_task;
        self.next_step_task = self
            .next_step_task
            .checked_add(1)
            .ok_or_else(|| HotSwapError::Vm("step task id overflow".to_owned()))?;
        self.step_tasks.insert(id, task);
        Ok(id)
    }

    /// Advances stepped task `id` by up to `steps` VM steps, pausing at a safe
    /// point where [`Self::swap_definitions`] and [`Self::migrate_state_cells`]
    /// may run cleanly.
    pub fn step_eval(
        &mut self,
        id: StepTaskId,
        steps: usize,
    ) -> Result<StepEvalOutcome, HotSwapError> {
        let Some(mut task) = self.step_tasks.remove(&id) else {
            return Err(HotSwapError::UnknownTask(id));
        };
        match &mut task {
            SuspendedTask::Tier0(t0) => {
                match step_tier0_n(
                    t0,
                    &mut self.heap,
                    self.program.program(),
                    &mut self.persistent,
                    steps,
                ) {
                    Tier0StepResult::Running { steps_taken } => {
                        self.step_tasks.insert(id, task);
                        Ok(StepEvalOutcome::Running { steps_taken })
                    }
                    Tier0StepResult::Finished(report) => {
                        let outcome = self.classify_value_report(report);
                        let pinned = self.all_pinned_codes();
                        self.program.reclaim_retired_codes(&pinned);
                        Ok(StepEvalOutcome::Finished(outcome))
                    }
                }
            }
            SuspendedTask::Tier1(t1) => {
                match t1.step_n(&mut self.heap, &mut self.persistent, steps) {
                    Tier1StepResult::Running { steps_taken } => {
                        self.step_tasks.insert(id, task);
                        Ok(StepEvalOutcome::Running { steps_taken })
                    }
                    Tier1StepResult::Finished(report) => {
                        let outcome = self.classify_value_report(report);
                        let pinned = self.all_pinned_codes();
                        self.program.reclaim_retired_codes(&pinned);
                        Ok(StepEvalOutcome::Finished(outcome))
                    }
                }
            }
        }
    }

    /// Runs stepped task `id` to completion (within its remaining fuel budget).
    pub fn finish_step_eval(&mut self, id: StepTaskId) -> Result<EvalOutcome, HotSwapError> {
        loop {
            match self.step_eval(id, 100_000)? {
                StepEvalOutcome::Running { .. } => {}
                StepEvalOutcome::Finished(outcome) => return Ok(outcome),
            }
        }
    }

    /// Aborts a paused stepped task and releases all of its heap roots cleanly.
    pub fn cancel_step_eval(&mut self, id: StepTaskId) -> Result<(), HotSwapError> {
        let Some(task) = self.step_tasks.remove(&id) else {
            return Err(HotSwapError::UnknownTask(id));
        };
        match task {
            SuspendedTask::Tier0(t0) => {
                abort_tier0_step(*t0, &mut self.heap, self.program.program())
                    .map_err(|e| HotSwapError::Vm(e.to_string()))
            }
            SuspendedTask::Tier1(t1) => t1
                .abort(&mut self.heap)
                .map_err(|e| HotSwapError::Vm(e.to_string())),
        }
    }

    /// Aborts all active paused stepped tasks during [`Self::shutdown`].
    pub(super) fn abort_all_step_tasks(&mut self) -> Result<(), Error> {
        let tasks = mem::take(&mut self.step_tasks);
        let mut first_err = None;
        for (_, task) in tasks {
            let res = match task {
                SuspendedTask::Tier0(t0) => {
                    abort_tier0_step(*t0, &mut self.heap, self.program.program())
                }
                SuspendedTask::Tier1(t1) => t1.abort(&mut self.heap),
            };
            if let Err(err) = res {
                first_err.get_or_insert(err);
            }
        }
        first_err.map_or(Ok(()), Err)
    }

    fn get_or_prepare_tier1(
        &mut self,
        entry: &Name,
        root: Option<&Name>,
    ) -> Result<Arc<Prepared>, HotSwapError> {
        let key = (entry.clone(), root.cloned());
        if let Some(cached) = self.tier1_cache.get(&key) {
            return Ok(Arc::clone(&cached.image));
        }
        let extra_codes = self.all_pinned_codes();
        let prepared = prepare_verified_tier1(self.program.program(), entry, root, &extra_codes)?;
        let deps: BTreeSet<Name> = prepared.global_names.iter().cloned().collect();
        let arc = Arc::new(prepared);
        self.tier1_cache.insert(
            key,
            CachedPrepared {
                image: Arc::clone(&arc),
                deps,
            },
        );
        Ok(arc)
    }

    /// Helper for internal tests/callers to inspect raw tier-1 stats of a cached entry.
    #[must_use]
    pub fn cached_tier1_stats(&self, entry: &Name) -> Option<PreparedStats> {
        self.tier1_cache
            .get(&(entry.clone(), None))
            .map(|c| c.image.stats())
    }
}

/// Evaluates `entry` directly on tier 0 and discards a returned `ProgramOutcome::Returned` value.
#[allow(dead_code)]
pub(super) fn drop_outcome_value(heap: &mut Heap, outcome: ProgramOutcome) -> Result<(), Error> {
    outcome.release(heap)
}
