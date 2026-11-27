//! `Lazy` cells (shared single-flight and local) and root cell/lazy cleanup for tier 1.

use std::mem;

use crate::tier0::{CauseToken, Data, Error, LazyHandle, LazyState, Reason, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_fibers::ParkReason;

impl Machine<'_, '_> {
    pub(super) fn exec_lazy_make(
        &mut self,
        dst: usize,
        task_slot: usize,
        local: bool,
    ) -> Result<(), Error> {
        let task = self.take_value(task_slot)?;
        let local_owner = local.then_some(self.active_fiber);
        let root = self.heap.allocate(Data::Lazy {
            execution: self.execution,
            local_owner,
            state: Some(LazyState::Pending(task)),
        });
        let id = root.object_id().expect("lazy allocation");
        self.lazies.push(root);
        self.store_reg(
            dst,
            RegVal::Value(Value::Lazy(LazyHandle {
                id,
                execution: self.execution,
            })),
        )
    }

    fn end_can_reset_local(&self, end: &End) -> bool {
        match end {
            End::Handle { activation, .. } => self
                .activations
                .get(activation)
                .is_none_or(|act| act.errors.is_some() || !act.operations.is_empty()),
            End::Try { .. }
            | End::CaptureExit { .. }
            | End::CatchDefect { .. }
            | End::CatchCause { .. }
            | End::EnsuringFinish { .. }
            | End::OnExitDecide { .. }
            | End::BracketRelease { .. }
            | End::CloseScope { .. }
            | End::ClosingScope { .. }
            | End::LayerRun { .. }
            | End::LayerBuildFinish { .. }
            | End::Discontinue { .. }
            | End::Invocation(_)
            | End::DiscardCleanupDone { .. }
            | End::OnExitDiscardRunFin { .. }
            | End::BracketFinish { .. }
            | End::EnsuringDone { .. }
            | End::OnExitRunFin { .. }
            | End::OnExitFinish { .. } => true,
            End::RestoreMask { next, .. }
            | End::WithServices { next, .. }
            | End::CachedComplete { next, .. } => self.end_can_reset_local(next),
            End::Top
            | End::Store(_)
            | End::Global { .. }
            | End::Modify { .. }
            | End::CliStep { .. }
            | End::FunctionBoundary { .. }
            | End::Loop(_)
            | End::GenerateStrict { .. }
            | End::GeneratorStep { .. }
            | End::LazyComplete { .. }
            | End::AcquireRegister { .. }
            | End::BracketUse { .. }
            | End::RestoreOwner { .. }
            | End::RestoreOwnerEvidence { .. }
            | End::RestoreFiberRef { .. }
            | End::RestoreEnvRefs { .. }
            | End::RestoreRandom { .. }
            | End::ProvideContext { .. }
            | End::OrDie { .. }
            | End::TimedFinish { .. }
            | End::RecoverySelection { .. }
            | End::RecoveryHelper { .. }
            | End::RecoveryBody { .. }
            | End::DiscardFinalizer => false,
        }
    }

    pub(super) fn exec_lazy_force(&mut self, dst: usize, lazy_slot: usize) -> Result<(), Error> {
        enum LazyAction {
            Return(Value),
            FailCause(CauseToken),
            Wait,
            Start(Value),
        }
        self.stage_slots(&[lazy_slot])?;
        let Value::Lazy(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedLazy);
        };
        let handle = LazyHandle {
            id: handle.id,
            execution: handle.execution,
        };
        self.release_operands()?;
        let active = self.active_fiber;
        let has_reset_frame = self
            .frames
            .iter()
            .any(|frame| self.end_can_reset_local(&frame.end));
        let action = {
            let (slot, local_owner) = self.heap.lazy_slot_mut(self.execution, &handle)?;
            match slot {
                Some(LazyState::DoneOk(value)) => {
                    let rep = value.representation();
                    LazyAction::Return(self.heap.dup(&rep)?)
                }
                Some(LazyState::DoneCause(cause)) => {
                    let id = cause.object_id();
                    LazyAction::FailCause(self.heap.dup_cause_id(id)?)
                }
                Some(LazyState::Running { owner, .. }) => {
                    if *owner == active {
                        return Err(Error::LazyCycle);
                    }
                    LazyAction::Wait
                }
                Some(LazyState::RunningLocal { .. }) => return Err(Error::LazyCycle),
                Some(LazyState::Pending(_)) => {
                    let Some(LazyState::Pending(task)) = slot.take() else {
                        unreachable!();
                    };
                    if let Some(creator) = local_owner {
                        if active != creator || has_reset_frame {
                            let rep = task.representation();
                            *slot = Some(LazyState::RunningLocal { task: Some(task) });
                            LazyAction::Start(self.heap.dup(&rep)?)
                        } else {
                            *slot = Some(LazyState::RunningLocal { task: None });
                            LazyAction::Start(task)
                        }
                    } else {
                        let rep = task.representation();
                        *slot = Some(LazyState::Running {
                            task,
                            owner: active,
                        });
                        LazyAction::Start(self.heap.dup(&rep)?)
                    }
                }
                None => return Err(Error::ClosedLazy),
            }
        };
        match action {
            LazyAction::Return(value) => self.store_reg(dst, RegVal::Value(value)),
            LazyAction::FailCause(cause) => {
                if let Some(prev) = self.failure.replace(cause) {
                    self.heap.drop_cause(prev)?;
                }
                self.failure_elected = true;
                Ok(())
            }
            LazyAction::Wait => {
                self.lazy_waiters
                    .entry(handle.id)
                    .or_default()
                    .push((active, dst));
                self.park_current(ParkReason::Lazy { lazy_id: handle.id })
            }
            LazyAction::Start(task) => {
                self.await_task_reg(RegVal::Value(task), End::LazyComplete { lazy: handle, dst })
            }
        }
    }

    pub(super) fn complete_lazy_ok(&mut self, lazy: &LazyHandle, dst: usize) -> Result<(), Error> {
        let reg = self.pending.take().expect("lazy result");
        let value = self.materialize_reg(reg);
        let cached = match self.heap.dup(&value) {
            Ok(dup) => dup,
            Err(err) => {
                self.heap.drop_value(value)?;
                return Err(err);
            }
        };
        let rep = cached.representation();
        let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
        let prev = slot.replace(LazyState::DoneOk(cached));
        if let Some(
            LazyState::Running { task, .. } | LazyState::RunningLocal { task: Some(task) },
        ) = prev
        {
            self.heap.drop_value(task)?;
        }
        self.store_reg(dst, RegVal::Value(value))?;
        if let Some(waiters) = self.lazy_waiters.remove(&lazy.id) {
            for (wfib, wdst) in waiters {
                let dup = self.heap.dup(&rep)?;
                self.wake_fiber_with_value(wfib, wdst, dup)?;
            }
        }
        Ok(())
    }

    pub(super) fn unwind_lazy(&mut self, lazy: &LazyHandle) -> Result<(), Error> {
        let cacheable_id = if let Some(cause) = self.failure.as_ref() {
            let has_int = self
                .heap
                .cause_reasons(cause)?
                .any(|r| matches!(r, Reason::Interrupt { .. }));
            if has_int {
                None
            } else {
                Some(cause.object_id())
            }
        } else {
            None
        };
        if let Some(cause_id) = cacheable_id {
            let cached = self.heap.dup_cause_id(cause_id)?;
            let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
            let prev = slot.replace(LazyState::DoneCause(cached));
            if let Some(
                LazyState::Running { task, .. } | LazyState::RunningLocal { task: Some(task) },
            ) = prev
            {
                self.heap.drop_value(task)?;
            }
            if let Some(waiters) = self.lazy_waiters.remove(&lazy.id) {
                for (wfib, _) in waiters {
                    let dup = self.heap.dup_cause_id(cause_id)?;
                    self.wake_fiber_with_failure(wfib, dup)?;
                }
            }
        } else {
            self.reset_lazy_or_handoff(lazy)?;
        }
        Ok(())
    }

    pub(super) fn reset_lazy_or_handoff(&mut self, lazy: &LazyHandle) -> Result<(), Error> {
        let mut next_waiter = None;
        if let Some(waiters) = self.lazy_waiters.get_mut(&lazy.id) {
            while !waiters.is_empty() {
                let (wfib, wdst) = waiters.remove(0);
                let still_parked = self.fiber_table.get(&wfib).is_some_and(|r| {
                    matches!(r.parked, Some(ParkReason::Lazy { lazy_id }) if lazy_id == lazy.id)
                });
                if still_parked {
                    next_waiter = Some((wfib, wdst));
                    break;
                }
            }
            if waiters.is_empty() {
                self.lazy_waiters.remove(&lazy.id);
            }
        }
        if let Some((wfib, wdst)) = next_waiter {
            let task_dup = {
                let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
                if let Some(LazyState::Running { task, owner }) = slot {
                    *owner = wfib;
                    let rep = task.representation();
                    Some(self.heap.dup(&rep)?)
                } else {
                    None
                }
            };
            if let Some(task_dup) = task_dup {
                if let Some(rec) = self.fiber_table.get_mut(&wfib) {
                    rec.parked = None;
                }
                let caller = self.active_fiber;
                let handle = LazyHandle {
                    id: lazy.id,
                    execution: lazy.execution,
                };
                self.switch_to_fiber(wfib);
                let res = self.await_task_reg(
                    RegVal::Value(task_dup),
                    End::LazyComplete {
                        lazy: handle,
                        dst: wdst,
                    },
                );
                self.switch_to_fiber(caller);
                res?;
                self.run_queue.push_back(wfib);
                return Ok(());
            }
        }
        let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
        if let Some(
            LazyState::Running { task, .. } | LazyState::RunningLocal { task: Some(task) },
        ) = slot.take()
        {
            *slot = Some(LazyState::Pending(task));
        }
        Ok(())
    }

    pub(super) fn detach_root_cells_and_lazies(
        &mut self,
        detached: &mut Vec<Value>,
        failure: &mut Option<Error>,
    ) {
        for root in &self.cells {
            let Value::Object(token) = root else {
                failure.get_or_insert(Error::ExpectedCell);
                continue;
            };
            match self.heap.object_mut(token.id).map(|obj| &mut obj.data) {
                Ok(Data::Cell { contents, .. }) => {
                    if let Some(value) = contents.take() {
                        if matches!(value, Value::Object(_)) {
                            self.heap.record_detached_cell();
                        }
                        detached.push(value);
                    }
                }
                Ok(_) => {
                    failure.get_or_insert(Error::ExpectedCell);
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        for root in &self.lazies {
            let Value::Object(token) = root else {
                failure.get_or_insert(Error::ExpectedLazy);
                continue;
            };
            match self.heap.object_mut(token.id).map(|obj| &mut obj.data) {
                Ok(Data::Lazy { state, .. }) => {
                    if let Some(prev) = state.take() {
                        match prev {
                            LazyState::Pending(v)
                            | LazyState::Running { task: v, .. }
                            | LazyState::RunningLocal { task: Some(v) }
                            | LazyState::DoneOk(v) => detached.push(v),
                            LazyState::DoneCause(c) => detached.push(c.into_value()),
                            LazyState::RunningLocal { task: None } => {}
                        }
                    }
                }
                Ok(_) => {
                    failure.get_or_insert(Error::ExpectedLazy);
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        detached.extend(mem::take(&mut self.cells));
        detached.extend(mem::take(&mut self.lazies));
    }
}
