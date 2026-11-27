//! Synchronous scope ownership and LIFO finalizer state machine for tier 1.

use std::mem;

use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{CauseToken, Data, DefectSpec, Error, ExitData, OwnerHandle, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_unwind::Discard;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnerPhase {
    Open,
    Closing,
    Finalizing,
    Closed,
}

pub(super) enum Finalizer {
    Task(RegVal),
    WithCause(RegVal),
    WithExit(RegVal),
    Release { value: Value, release_fn: RegVal },
}

pub(super) struct OwnerState {
    pub(super) phase: OwnerPhase,
    pub(super) exit_cause: Option<CauseToken>,
    pub(super) finalizers: Vec<Finalizer>,
}

impl Default for OwnerState {
    fn default() -> Self {
        Self {
            phase: OwnerPhase::Open,
            exit_cause: None,
            finalizers: Vec::new(),
        }
    }
}

impl OwnerState {
    pub(super) fn is_finalizing(&self) -> bool {
        self.phase == OwnerPhase::Finalizing
    }

    pub(super) fn is_finalizing_or_closed(&self) -> bool {
        matches!(self.phase, OwnerPhase::Finalizing | OwnerPhase::Closed)
    }

    pub(super) fn is_closed(&self) -> bool {
        self.phase == OwnerPhase::Closed
    }
}

pub(super) enum BracketOutcome {
    Success(RegVal),
    Failure(CauseToken),
}

pub(super) enum ScopeOutcome {
    Normal { dst: usize, value: RegVal },
    Failure(CauseToken),
    Discard(Box<Discard>),
}

pub(super) struct ClosingScopeState {
    pub(super) owner: usize,
    pub(super) previous: usize,
    pub(super) remaining: Vec<Finalizer>,
    pub(super) outcome: ScopeOutcome,
    pub(super) exit_override: Option<CauseToken>,
    pub(super) finalizer_failure: Option<CauseToken>,
    pub(super) saved_mask: usize,
    pub(super) masked: bool,
}

impl Machine<'_, '_> {
    pub(super) fn owner_handle(&self, index: usize) -> OwnerHandle {
        OwnerHandle {
            heap: self.heap.identity,
            execution: self.execution,
            index,
        }
    }

    pub(super) fn finalizer_roots(finalizer: Finalizer, roots: &mut Vec<Value>) {
        match finalizer {
            Finalizer::Task(r) | Finalizer::WithCause(r) | Finalizer::WithExit(r) => {
                Self::reg_into_roots(r, roots);
            }
            Finalizer::Release { value, release_fn } => {
                roots.push(value);
                Self::reg_into_roots(release_fn, roots);
            }
        }
    }

    pub(super) fn exec_scope_run(&mut self, dst: usize, body_slot: usize) -> Result<(), Error> {
        let body = self.take_reg(body_slot)?;
        let previous = self.current_owner;
        let owner = self.owners.len();
        self.owners.push(OwnerState::default());
        self.current_owner = owner;
        let handle = Value::Owner(self.owner_handle(owner));
        self.apply_reg(
            body,
            RegVal::Value(handle),
            End::CloseScope {
                owner,
                previous,
                dst,
            },
            false,
        )
    }

    pub(super) fn exec_scope_current(&mut self, dst: usize, unit_slot: usize) -> Result<(), Error> {
        let unit = self.take_reg(unit_slot)?;
        self.drop_reg(unit)?;
        let handle = Value::Owner(self.owner_handle(self.current_owner));
        self.store_raw(dst, RegVal::Value(handle));
        Ok(())
    }

    pub(super) fn exec_scope_add_finalizer(
        &mut self,
        dst: usize,
        task_slot: usize,
    ) -> Result<(), Error> {
        let task = self.take_reg(task_slot)?;
        self.register_scope_finalizer(dst, Finalizer::Task(task))
    }

    pub(super) fn exec_scope_add_finalizer_exit(
        &mut self,
        dst: usize,
        func_slot: usize,
    ) -> Result<(), Error> {
        let func = self.take_reg(func_slot)?;
        self.register_scope_finalizer(dst, Finalizer::WithCause(func))
    }

    pub(super) fn exec_scope_add_close(
        &mut self,
        dst: usize,
        func_slot: usize,
    ) -> Result<(), Error> {
        let func = self.take_reg(func_slot)?;
        self.register_scope_finalizer(dst, Finalizer::WithExit(func))
    }

    fn register_scope_finalizer(&mut self, dst: usize, finalizer: Finalizer) -> Result<(), Error> {
        let owner = self.current_owner;
        if self.owners[owner].is_closed() {
            Self::finalizer_roots(finalizer, &mut self.scratch);
            return self.raise_defect(DefectSpec::arena_owner_closed());
        }
        if self.owners[owner].is_finalizing() {
            let exit_cause = match &self.owners[owner].exit_cause {
                Some(c) => Some(self.heap.dup_cause(c)?),
                None => None,
            };
            let saved_mask = self.mask_depth;
            self.mask_depth += 1;
            let next = End::RestoreMask {
                saved_mask,
                next: Box::new(End::Store(dst)),
            };
            let res = self.run_single_finalizer(finalizer, exit_cause.as_ref(), next);
            if let Some(c) = exit_cause {
                self.heap.drop_cause(c)?;
            }
            return res;
        }
        self.owners[owner].finalizers.push(finalizer);
        self.store_raw(dst, RegVal::Value(Value::Unit));
        Ok(())
    }

    pub(super) fn owner_needs_close_work(&self, owner: usize) -> bool {
        let o = &self.owners[owner];
        !o.is_closed() && !o.finalizers.is_empty()
    }

    pub(super) fn begin_close_scope(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome,
        exit_override: Option<CauseToken>,
    ) -> Result<(), Error> {
        if !self.owner_needs_close_work(owner) {
            if !self.owners[owner].is_closed() {
                self.owners[owner].phase = OwnerPhase::Closing;
            }
            if let Some(c) = exit_override {
                self.heap.drop_cause(c)?;
            }
            self.close_owner(owner)?;
            self.current_owner = previous;
            return self.complete_scope_outcome(outcome, None);
        }
        let exit_cause = Self::outcome_exit_cause(exit_override.as_ref(), &outcome);
        let exit_dup = match exit_cause {
            Some(c) => Some(self.heap.dup_cause(c)?),
            None => None,
        };
        let o = &mut self.owners[owner];
        o.phase = OwnerPhase::Finalizing;
        o.exit_cause = exit_dup;
        let remaining = mem::take(&mut o.finalizers);
        self.current_owner = owner;
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        let state = Box::new(ClosingScopeState {
            owner,
            previous,
            remaining,
            outcome,
            exit_override,
            finalizer_failure: None,
            saved_mask,
            masked: true,
        });
        self.step_close_scope(state)
    }

    fn outcome_exit_cause<'a>(
        exit_override: Option<&'a CauseToken>,
        outcome: &'a ScopeOutcome,
    ) -> Option<&'a CauseToken> {
        exit_override.or(match outcome {
            ScopeOutcome::Failure(c) => Some(c),
            ScopeOutcome::Discard(discard) => Self::peek_discard_exit_cause(&discard.next),
            ScopeOutcome::Normal { .. } => None,
        })
    }

    pub(super) fn step_close_scope(
        &mut self,
        mut state: Box<ClosingScopeState>,
    ) -> Result<(), Error> {
        self.current_owner = state.owner;
        let Some(finalizer) = state.remaining.pop() else {
            let owner = state.owner;
            let previous = state.previous;
            let outcome = state.outcome;
            let finalizer_failure = state.finalizer_failure;
            if let Some(c) = state.exit_override {
                self.heap.drop_cause(c)?;
            }
            if state.masked {
                self.mask_depth = state.saved_mask;
            }
            self.close_owner(owner)?;
            self.current_owner = previous;
            return self.complete_scope_outcome(outcome, finalizer_failure);
        };
        let exit_cause =
            match Self::outcome_exit_cause(state.exit_override.as_ref(), &state.outcome) {
                Some(c) => Some(self.heap.dup_cause(c)?),
                None => None,
            };
        let res =
            self.run_single_finalizer(finalizer, exit_cause.as_ref(), End::ClosingScope { state });
        if let Some(c) = exit_cause {
            self.heap.drop_cause(c)?;
        }
        res
    }

    fn run_single_finalizer(
        &mut self,
        finalizer: Finalizer,
        exit_cause: Option<&CauseToken>,
        next: End,
    ) -> Result<(), Error> {
        match finalizer {
            Finalizer::Task(task) => self.await_task_reg(task, next),
            Finalizer::WithCause(func) => {
                let arg = match exit_cause {
                    Some(cause) => {
                        let dup = self.heap.dup_cause(cause)?;
                        self.heap
                            .construct("Prelude.Option.Some", vec![dup.into_value()])?
                    }
                    None => self.heap.construct("Prelude.Option.None", vec![])?,
                };
                self.apply_reg(func, RegVal::Value(arg), next, false)
            }
            Finalizer::WithExit(func) => {
                let exit = match exit_cause {
                    Some(cause) => {
                        let dup = self.heap.dup_cause(cause)?;
                        self.heap
                            .allocate(Data::Exit(ExitData::Failure(dup.into_value())))
                    }
                    None => self
                        .heap
                        .allocate(Data::Exit(ExitData::Success(Value::Unit))),
                };
                self.apply_reg(func, RegVal::Value(exit), next, false)
            }
            Finalizer::Release { value, release_fn } => {
                self.apply_reg(release_fn, RegVal::Value(value), next, false)
            }
        }
    }

    pub(super) fn finalizer_failed(
        &mut self,
        mut state: Box<ClosingScopeState>,
    ) -> Result<(), Error> {
        let cause = self.failure.take().expect("finalizer failure cause");
        state.finalizer_failure = Some(match state.finalizer_failure.take() {
            None => cause,
            Some(prev) => {
                let combined = primitives::concat_causes(
                    self.heap,
                    &prev,
                    &cause,
                    &mut FuelCounter(&mut self.fuel),
                );
                self.heap.drop_cause(prev)?;
                self.heap.drop_cause(cause)?;
                match combined {
                    Ok(c) => c,
                    Err(err) => {
                        let mut roots = Vec::new();
                        self.drain_closing_scope_roots(*state, &mut roots);
                        self.heap.drop_values(roots)?;
                        return Err(err);
                    }
                }
            }
        });
        self.step_close_scope(state)
    }

    fn complete_scope_outcome(
        &mut self,
        outcome: ScopeOutcome,
        finalizer_failure: Option<CauseToken>,
    ) -> Result<(), Error> {
        match outcome {
            ScopeOutcome::Normal { dst, value } => {
                if let Some(cause) = finalizer_failure {
                    self.drop_reg(value)?;
                    self.failure = Some(cause);
                    self.failure_elected = true;
                    Ok(())
                } else {
                    self.store_reg(dst, value)
                }
            }
            ScopeOutcome::Failure(cause) => {
                let joined = if let Some(fin_cause) = finalizer_failure {
                    let res = primitives::join_causes(
                        self.heap,
                        &cause,
                        &fin_cause,
                        &mut FuelCounter(&mut self.fuel),
                    );
                    self.heap.drop_cause(cause)?;
                    self.heap.drop_cause(fin_cause)?;
                    res?
                } else {
                    cause
                };
                self.failure = Some(joined);
                self.failure_elected = true;
                Ok(())
            }
            ScopeOutcome::Discard(discard) => {
                if let Some(fin_cause) = finalizer_failure {
                    self.escalate_paused_discard(*discard, fin_cause)
                } else {
                    self.discard = Some(*discard);
                    Ok(())
                }
            }
        }
    }

    pub(super) fn close_owner(&mut self, owner: usize) -> Result<(), Error> {
        let state = &mut self.owners[owner];
        if state.is_closed() {
            return Ok(());
        }
        state.phase = OwnerPhase::Closed;
        let mut roots = Vec::new();
        if let Some(c) = state.exit_cause.take() {
            roots.push(c.into_value());
        }
        for fin in mem::take(&mut state.finalizers) {
            Self::finalizer_roots(fin, &mut roots);
        }
        self.heap.drop_values(roots)
    }
}
