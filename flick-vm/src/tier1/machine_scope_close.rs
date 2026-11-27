//! C0-C6 scope closing state machine, hooks, LIFO finalizers, and outcome
//! completion for tier 1.

use std::mem;

use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{CauseToken, Data, Error, ExitData, Reason, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_fibers::ParkReason;
use super::machine_scopes::{
    ClosePhase, ClosingScopeState, Finalizer, OwnerHook, OwnerPhase, ScopeOutcome,
};

impl Machine<'_, '_> {
    pub(super) fn begin_close_scope(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome,
        exit_override: Option<CauseToken>,
    ) -> Result<(), Error> {
        self.begin_close_scope_with_override(owner, previous, outcome, exit_override, false)
    }

    pub(super) fn begin_close_scope_elect(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome,
        elect_on_fail: bool,
    ) -> Result<(), Error> {
        self.begin_close_scope_with_override(owner, previous, outcome, None, elect_on_fail)
    }

    pub(super) fn begin_close_scope_with_override(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome,
        exit_override: Option<CauseToken>,
        elect_on_fail: bool,
    ) -> Result<(), Error> {
        if !self.owner_needs_async_close(owner) {
            if !self.owners[owner].is_closed() {
                self.owners[owner].phase = OwnerPhase::Closing;
            }
            if let Some(c) = exit_override {
                self.heap.drop_cause(c)?;
            }
            self.close_owner(owner)?;
            self.current_owner = previous;
            return self.complete_scope_outcome(outcome, None, elect_on_fail);
        }
        self.owners[owner].phase = OwnerPhase::Closing;
        self.current_owner = owner;
        let state = Box::new(ClosingScopeState {
            owner,
            previous,
            phase: ClosePhase::Hooks,
            remaining: Vec::new(),
            outcome,
            exit_override,
            finalizer_failure: None,
            elect_on_fail,
            saved_mask: self.mask_depth,
            masked: false,
        });
        self.step_close_scope(state)
    }

    pub(super) fn resume_owner_close(&mut self, owner: usize) -> Result<(), Error> {
        let Some(state) = self.closing_scopes.remove(&owner) else {
            return Ok(());
        };
        self.step_close_scope(state)
    }

    fn state_exit_cause(state: &ClosingScopeState) -> Option<&CauseToken> {
        state.exit_override.as_ref().or(match &state.outcome {
            ScopeOutcome::Failure(c) => Some(c),
            ScopeOutcome::ChildClose { inherited_exit, .. } => inherited_exit.as_ref(),
            ScopeOutcome::Discard(discard) => Self::peek_discard_exit_cause(&discard.next),
            ScopeOutcome::Normal { .. } => None,
        })
    }

    pub(super) fn step_close_scope(
        &mut self,
        mut state: Box<ClosingScopeState>,
    ) -> Result<(), Error> {
        loop {
            self.current_owner = state.owner;
            match state.phase {
                ClosePhase::Hooks | ClosePhase::PostHooks => {
                    let Some(next) = self.step_close_hooks(state)? else {
                        return Ok(());
                    };
                    state = next;
                }
                ClosePhase::Children => {
                    let children: Vec<usize> =
                        self.owners[state.owner].children.iter().copied().collect();
                    let me = self.active_fiber as u64;
                    for child in children {
                        if let Err(err) = self.request_interrupt(child, me) {
                            self.closing_scopes.insert(state.owner, state);
                            return Err(err);
                        }
                    }
                    if self.owners[state.owner].watchers > 0 {
                        let owner = state.owner;
                        self.owners[owner].close_waiter = Some(self.active_fiber);
                        self.closing_scopes.insert(owner, state);
                        return self.park_current(ParkReason::OwnerChildren { owner });
                    }
                    state.phase = ClosePhase::PostHooks;
                }
                ClosePhase::Finalizers => {
                    let Some(next) = self.step_close_finalizer(state)? else {
                        return Ok(());
                    };
                    state = next;
                }
            }
        }
    }

    fn step_close_hooks(
        &mut self,
        mut state: Box<ClosingScopeState>,
    ) -> Result<Option<Box<ClosingScopeState>>, Error> {
        if let Some(OwnerHook::StopFiberGroup(gid)) = self.owners[state.owner].hooks.pop_front() {
            let wait = match self.stop_fiber_group_for_close(gid, state.owner) {
                Ok(w) => w,
                Err(err) => {
                    self.closing_scopes.insert(state.owner, state);
                    return Err(err);
                }
            };
            if wait.is_some() {
                let owner = state.owner;
                self.owners[owner].close_waiter = Some(self.active_fiber);
                self.closing_scopes.insert(owner, state);
                self.park_current(ParkReason::OwnerHook { owner })?;
                return Ok(None);
            }
            return Ok(Some(state));
        }
        if state.phase == ClosePhase::Hooks {
            state.phase = ClosePhase::Children;
        } else {
            let owner = state.owner;
            if !self.owners[owner].hooks.is_empty() {
                return Ok(Some(state));
            }
            if !self.owners[owner].children.is_empty() || self.owners[owner].watchers > 0 {
                state.phase = ClosePhase::Children;
                return Ok(Some(state));
            }
            let exit_dup = match Self::state_exit_cause(&state) {
                Some(c) => match self.heap.dup_cause(c) {
                    Ok(d) => Some(d),
                    Err(err) => {
                        self.closing_scopes.insert(owner, state);
                        return Err(err);
                    }
                },
                None => None,
            };
            let o = &mut self.owners[owner];
            o.phase = OwnerPhase::Finalizing;
            o.exit_cause = exit_dup;
            state.remaining = mem::take(&mut o.finalizers);
            state.saved_mask = self.mask_depth;
            self.mask_depth += 1;
            state.masked = true;
            state.phase = ClosePhase::Finalizers;
        }
        Ok(Some(state))
    }

    fn step_close_finalizer(
        &mut self,
        mut state: Box<ClosingScopeState>,
    ) -> Result<Option<Box<ClosingScopeState>>, Error> {
        let Some(finalizer) = state.remaining.pop() else {
            let owner = state.owner;
            let previous = state.previous;
            let outcome = state.outcome;
            let finalizer_failure = state.finalizer_failure;
            let elect_on_fail = state.elect_on_fail;
            if let Some(c) = state.exit_override {
                self.heap.drop_cause(c)?;
            }
            if state.masked {
                self.mask_depth = state.saved_mask;
            }
            self.close_owner(owner)?;
            self.current_owner = previous;
            self.complete_scope_outcome(outcome, finalizer_failure, elect_on_fail)?;
            return Ok(None);
        };
        match finalizer {
            Finalizer::CloseChildOwner(child_owner) => {
                if self.owners[child_owner].is_closed() {
                    return Ok(Some(state));
                }
                self.close_child_owner_for_finalizer(child_owner, state)?;
                Ok(None)
            }
            Finalizer::ReleaseMemo {
                memo_id,
                layer_id,
                entry_owner,
            } => {
                if let Some(scope_owner) =
                    self.release_memo_observer(memo_id, layer_id, entry_owner)
                    && !self.owners[scope_owner].is_closed()
                {
                    self.close_child_owner_for_finalizer(scope_owner, state)?;
                    return Ok(None);
                }
                Ok(Some(state))
            }
            user_fin => {
                let exit_cause = match Self::state_exit_cause(&state) {
                    Some(c) => Some(self.heap.dup_cause(c)?),
                    None => None,
                };
                let res = self.run_single_finalizer(
                    user_fin,
                    exit_cause.as_ref(),
                    End::ClosingScope { state },
                );
                if let Some(c) = exit_cause {
                    self.heap.drop_cause(c)?;
                }
                res.map(|()| None)
            }
        }
    }

    fn close_child_owner_for_finalizer(
        &mut self,
        child_owner: usize,
        state: Box<ClosingScopeState>,
    ) -> Result<(), Error> {
        let inherited_exit = match Self::state_exit_cause(&state) {
            Some(c) => Some(self.heap.dup_cause(c)?),
            None => None,
        };
        let parent_owner = state.owner;
        self.begin_close_scope_elect(
            child_owner,
            parent_owner,
            ScopeOutcome::ChildClose {
                parent: state,
                inherited_exit,
            },
            false,
        )
    }

    pub(super) fn run_single_finalizer(
        &mut self,
        finalizer: Finalizer,
        exit_cause: Option<&CauseToken>,
        next: End,
    ) -> Result<(), Error> {
        let wrap = |services: Vec<Value>, inner: End| {
            if services.is_empty() {
                inner
            } else {
                End::WithServices {
                    services,
                    next: Box::new(inner),
                }
            }
        };
        match finalizer {
            Finalizer::Task { task, services } => {
                let end = wrap(services, next);
                self.bump_handler_epoch();
                self.await_task_reg(task, end)
            }
            Finalizer::WithCause { func, services } => {
                let arg = match exit_cause {
                    Some(cause) => {
                        let dup = self.heap.dup_cause(cause)?;
                        self.heap
                            .construct("Prelude.Option.Some", vec![dup.into_value()])?
                    }
                    None => self.heap.construct("Prelude.Option.None", vec![])?,
                };
                let end = wrap(services, next);
                self.bump_handler_epoch();
                self.apply_reg(func, RegVal::Value(arg), end, false)
            }
            Finalizer::WithExit { func, services } => {
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
                let end = wrap(services, next);
                self.bump_handler_epoch();
                self.apply_reg(func, RegVal::Value(exit), end, false)
            }
            Finalizer::Release {
                value,
                release_fn,
                services,
            } => {
                let end = wrap(services, next);
                self.bump_handler_epoch();
                self.apply_reg(release_fn, RegVal::Value(value), end, false)
            }
            Finalizer::CloseChildOwner(_) | Finalizer::ReleaseMemo { .. } => {
                unreachable!("structural finalizers handled in step_close_finalizer")
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

    fn attach_pending_interrupt(
        &mut self,
        mut cause: CauseToken,
        elect_on_fail: bool,
    ) -> Result<CauseToken, Error> {
        if !elect_on_fail
            && self.mask_depth == 0
            && let Some(by) = self.interrupt_requested
        {
            let has_int = self
                .heap
                .cause_reasons(&cause)?
                .any(|r| matches!(r, Reason::Interrupt { .. }));
            if !has_int {
                let int_c = match self.interrupt_cause(by) {
                    Ok(c) => c,
                    Err(err) => {
                        self.heap.drop_cause(cause)?;
                        return Err(err);
                    }
                };
                let res = primitives::join_causes(
                    self.heap,
                    &cause,
                    &int_c,
                    &mut FuelCounter(&mut self.fuel),
                );
                self.heap.drop_cause(cause)?;
                self.heap.drop_cause(int_c)?;
                cause = res?;
            }
        }
        Ok(cause)
    }

    pub(super) fn complete_scope_outcome(
        &mut self,
        outcome: ScopeOutcome,
        finalizer_failure: Option<CauseToken>,
        elect_on_fail: bool,
    ) -> Result<(), Error> {
        match outcome {
            ScopeOutcome::ChildClose {
                mut parent,
                inherited_exit,
            } => {
                if let Some(c) = inherited_exit {
                    self.heap.drop_cause(c)?;
                }
                if let Some(cf) = finalizer_failure {
                    parent.finalizer_failure = Some(match parent.finalizer_failure.take() {
                        None => cf,
                        Some(prev) => {
                            let comb = primitives::concat_causes(
                                self.heap,
                                &prev,
                                &cf,
                                &mut FuelCounter(&mut self.fuel),
                            );
                            self.heap.drop_cause(prev)?;
                            self.heap.drop_cause(cf)?;
                            match comb {
                                Ok(c) => c,
                                Err(err) => {
                                    self.closing_scopes.insert(parent.owner, parent);
                                    return Err(err);
                                }
                            }
                        }
                    });
                }
                self.step_close_scope(parent)
            }
            ScopeOutcome::Normal { dst, value } => {
                if let Some(cause) = finalizer_failure {
                    self.drop_reg(value)?;
                    let cause = self.attach_pending_interrupt(cause, elect_on_fail)?;
                    self.failure = Some(cause);
                    self.failure_elected = !elect_on_fail;
                } else if !elect_on_fail {
                    match self.check_active_interrupt() {
                        Ok(true) => self.drop_reg(value)?,
                        Ok(false) => {
                            if let Some(dst) = dst {
                                self.store_reg(dst, value)?;
                            } else {
                                self.top = Some(value);
                            }
                        }
                        Err(err) => {
                            self.drop_reg(value)?;
                            return Err(err);
                        }
                    }
                } else if let Some(dst) = dst {
                    self.store_reg(dst, value)?;
                } else {
                    self.top = Some(value);
                }
                Ok(())
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
                let joined = self.attach_pending_interrupt(joined, elect_on_fail)?;
                self.failure = Some(joined);
                self.failure_elected = !elect_on_fail;
                Ok(())
            }
            ScopeOutcome::Discard(discard) => {
                if let Some(fin_cause) = finalizer_failure {
                    self.escalate_paused_discard(*discard, fin_cause, elect_on_fail)
                } else {
                    self.discard = Some(*discard);
                    Ok(())
                }
            }
        }
    }
}
