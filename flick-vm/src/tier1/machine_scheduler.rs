//! Cooperative scheduler loop, timers, `TestClock` stepping, child completion,
//! and async root cleanup for tier 1.

use std::mem;

use crate::stdlib::{FIBER_QUANTUM, FuelCounter, QUANTUM_VIRTUAL_NS, primitives};
use crate::tier0::{CauseToken, Data, Error, ExitData, Value};

use super::machine::{End, Machine, OwnerEvidence};
use super::machine_fibers::{
    FiberContext, FiberOutcome, FiberWaitKind, FiberWaiter, ParkReason, TimerAction, TimerEntry,
};
use super::machine_layers::PendingProvideResult;

impl Machine<'_, '_> {
    pub(super) fn schedule_next(&mut self, active_finished: bool) -> Result<(), Error> {
        loop {
            if let Some(next_id) = self.run_queue.pop_front() {
                if !self.parked_fibers.contains_key(&next_id) {
                    if next_id == self.active_fiber && !active_finished {
                        self.quantum = FIBER_QUANTUM;
                        return Ok(());
                    }
                    continue;
                }
                if active_finished {
                    let mut next_ctx = self
                        .parked_fibers
                        .remove(&next_id)
                        .expect("next fiber context");
                    self.swap_fiber_context(&mut next_ctx);
                    let mut roots = Vec::new();
                    self.drain_fiber_ctx_roots(next_ctx, &mut roots);
                    self.heap.drop_values(roots)?;
                    self.active_fiber = next_id;
                    self.quantum = FIBER_QUANTUM;
                } else {
                    self.switch_to_fiber(next_id);
                }
                return Ok(());
            }
            if self.test_clock_mode {
                if self.advance_test_clock_step()? {
                    continue;
                }
            } else if self.fire_next_timer()? {
                continue;
            }
            if active_finished
                || self
                    .fiber_table
                    .get(&self.active_fiber)
                    .is_some_and(|r| r.parked.is_some())
            {
                return Err(Error::UnsupportedOperation(
                    "deadlock: no runnable fibers remain",
                ));
            }
            return Ok(());
        }
    }

    fn prevent_scheduler_yield_active(&self) -> bool {
        matches!(
            self.fiber_refs.get("flick/Scheduler/PreventYield"),
            Some(Value::Bool(true))
        )
    }

    fn active_max_ops_before_yield(&self) -> u32 {
        match self.fiber_refs.get("flick/Scheduler/MaxOpsBeforeYield") {
            Some(Value::Int(n)) if *n > 0 => u32::try_from(*n).unwrap_or(FIBER_QUANTUM),
            _ => FIBER_QUANTUM,
        }
    }

    pub(super) fn maybe_quantum_preempt(&mut self) -> Result<(), Error> {
        if self.interrupt_requested.is_some()
            && !self
                .frames
                .iter()
                .any(|frame| matches!(frame.end, End::Modify { .. } | End::Global { .. }))
            && self.check_active_interrupt()?
        {
            return Ok(());
        }
        if self.quantum > 0 {
            self.quantum -= 1;
            return Ok(());
        }
        self.quantum = self.active_max_ops_before_yield();
        if self
            .frames
            .iter()
            .any(|frame| matches!(frame.end, End::Modify { .. } | End::Global { .. }))
        {
            return Ok(());
        }
        if self.check_active_interrupt()? {
            return Ok(());
        }
        if self.prevent_scheduler_yield_active() {
            return Ok(());
        }
        if !self.test_clock_mode && !self.timers.is_empty() {
            self.virtual_now_ns = self.virtual_now_ns.saturating_add(QUANTUM_VIRTUAL_NS);
            self.fire_due_timers()?;
            if self.check_active_interrupt()? {
                return Ok(());
            }
        }
        if !self.run_queue.is_empty() {
            let fid = self.active_fiber;
            self.run_queue.push_back(fid);
            self.schedule_next(false)?;
        }
        Ok(())
    }

    pub(super) fn register_timer(&mut self, delay_ns: u64, action: TimerAction) -> usize {
        let id = self.next_timer;
        self.next_timer = self.next_timer.checked_add(1).expect("timer id overflow");
        let deadline_ns = self.virtual_now_ns.saturating_add(delay_ns);
        self.timers.insert(
            (deadline_ns, id),
            TimerEntry {
                id,
                deadline_ns,
                action,
            },
        );
        id
    }

    pub(super) fn cancel_timer(&mut self, timer_id: usize) {
        let key = self
            .timers
            .iter()
            .find(|(_, entry)| entry.id == timer_id)
            .map(|(&k, _)| k);
        if let Some(k) = key {
            self.timers.remove(&k);
        }
    }

    fn fire_due_timers(&mut self) -> Result<bool, Error> {
        let mut fired = false;
        while let Some((&(deadline, _), _)) = self.timers.first_key_value() {
            if deadline > self.virtual_now_ns {
                break;
            }
            let (_, entry) = self.timers.pop_first().expect("first timer");
            self.dispatch_timer(&entry)?;
            fired = true;
        }
        Ok(fired)
    }

    fn fire_next_timer(&mut self) -> Result<bool, Error> {
        let Some((_, entry)) = self.timers.pop_first() else {
            return Ok(false);
        };
        if entry.deadline_ns > self.virtual_now_ns {
            self.virtual_now_ns = entry.deadline_ns;
        }
        self.dispatch_timer(&entry)?;
        self.fire_due_timers()?;
        Ok(true)
    }

    fn dispatch_timer(&mut self, entry: &TimerEntry) -> Result<(), Error> {
        match entry.action {
            TimerAction::WakeSleep { fiber, dst } => {
                let matches = self.fiber_table.get(&fiber).is_some_and(|r| {
                    matches!(r.parked, Some(ParkReason::Sleep { timer_id }) if timer_id == entry.id)
                });
                if matches {
                    self.wake_fiber_with_value(fiber, dst, Value::Unit)?;
                }
            }
            TimerAction::FireTimeout { coord_id } => {
                self.on_timeout_timer(coord_id)?;
            }
        }
        Ok(())
    }

    fn advance_test_clock_step(&mut self) -> Result<bool, Error> {
        let Some(adv) = self.test_clock_waiter.as_ref() else {
            return Ok(false);
        };
        let target_ns = adv.target_ns;
        if let Some((&(deadline, _), _)) = self.timers.first_key_value()
            && deadline <= target_ns
        {
            if deadline > self.virtual_now_ns {
                self.virtual_now_ns = deadline;
            }
            self.fire_due_timers()?;
            return Ok(!self.run_queue.is_empty());
        }
        let adv = self.test_clock_waiter.take().expect("clock waiter");
        if target_ns > self.virtual_now_ns {
            self.virtual_now_ns = target_ns;
        }
        self.wake_fiber_with_value(adv.caller_fiber, adv.dst, Value::Unit)?;
        Ok(!self.run_queue.is_empty())
    }

    pub(super) fn combine_root_defects(
        &mut self,
        base: Option<CauseToken>,
    ) -> Result<Option<CauseToken>, Error> {
        let mut acc = base;
        while !self.root_defects.is_empty() {
            let defect = self.root_defects.remove(0);
            acc = Some(match acc {
                None => defect,
                Some(prev) => {
                    let res = primitives::join_causes(
                        self.heap,
                        &prev,
                        &defect,
                        &mut FuelCounter(&mut self.fuel),
                    );
                    self.heap.drop_cause(prev)?;
                    self.heap.drop_cause(defect)?;
                    res?
                }
            });
        }
        Ok(acc)
    }

    fn record_child_fiber_completion(
        &mut self,
        fid: usize,
        outcome: FiberOutcome,
    ) -> Result<(), Error> {
        let Some(rec) = self.fiber_table.get_mut(&fid) else {
            return self.drop_fiber_outcome(outcome);
        };
        let waiters = mem::take(&mut rec.waiters);
        let supervisor = rec.supervisor.take();
        let coord_parent = rec.coord_parent.take();
        let group_watchers = mem::take(&mut rec.group_watchers);
        let sup_owner = rec.is_supervisor_of.take();
        let child_owner = rec.owner.take();
        rec.outcome = outcome;

        if let Some(owner) = sup_owner {
            if let FiberOutcome::Failure(cause) = &self.fiber_table[&fid].outcome {
                let dup = self.heap.dup_cause(cause)?;
                self.root_defects.push(dup);
            }
            self.owners[owner].watchers = self.owners[owner].watchers.saturating_sub(1);
            if self.owners[owner].watchers == 0 {
                self.wake_owner_closer(owner);
            }
        }

        if let Some(owner) = child_owner {
            self.owners[owner].children.remove(&fid);
            if let Some(sup) = supervisor {
                if let FiberOutcome::Failure(cause) = &self.fiber_table[&fid].outcome {
                    let dup = self.heap.dup_cause(cause)?;
                    let sup_id = self.spawn_applied_fiber(
                        sup.owner,
                        Some(OwnerEvidence::from_owner(&self.owner_handle(sup.owner))),
                        sup.callback,
                        dup.into_value(),
                        sup.services,
                        1,
                    )?;
                    if let Some(srec) = self.fiber_table.get_mut(&sup_id) {
                        srec.is_supervisor_of = Some(owner);
                    }
                } else {
                    self.heap.drop_value(sup.callback)?;
                    self.heap.drop_values(sup.services)?;
                    self.owners[owner].watchers = self.owners[owner].watchers.saturating_sub(1);
                    if self.owners[owner].watchers == 0 {
                        self.wake_owner_closer(owner);
                    }
                }
            } else {
                self.owners[owner].watchers = self.owners[owner].watchers.saturating_sub(1);
                if self.owners[owner].watchers == 0 {
                    self.wake_owner_closer(owner);
                }
            }
        } else if let Some(sup) = supervisor {
            self.heap.drop_value(sup.callback)?;
            self.heap.drop_values(sup.services)?;
        }

        for (group_id, entry_key) in group_watchers {
            self.on_group_child_done(group_id, entry_key, fid)?;
        }
        if let Some((coord_id, idx)) = coord_parent {
            self.on_coord_child_done(coord_id, idx, fid)?;
        }
        for waiter in waiters {
            self.notify_fiber_waiter(fid, waiter)?;
        }
        Ok(())
    }

    pub(super) fn complete_child_fiber(&mut self, outcome: FiberOutcome) -> Result<(), Error> {
        let fid = self.active_fiber;
        self.record_child_fiber_completion(fid, outcome)?;
        self.schedule_next(true)
    }

    fn notify_fiber_waiter(&mut self, child_id: usize, waiter: FiberWaiter) -> Result<(), Error> {
        if let FiberWaitKind::CoordDone { coord_id, index } = waiter.kind {
            return self.on_coord_child_done(coord_id, index, child_id);
        }
        let is_waiting = self.fiber_table.get(&waiter.fiber).is_some_and(
            |r| matches!(r.parked, Some(ParkReason::Fiber { target }) if target == child_id),
        );
        if !is_waiting {
            return Ok(());
        }
        match waiter.kind {
            FiberWaitKind::Interrupt(dst) => {
                self.wake_fiber_with_value(waiter.fiber, dst, Value::Unit)
            }
            FiberWaitKind::Join(dst) => match &self.fiber_table[&child_id].outcome {
                FiberOutcome::Success(v) => {
                    let rep = v.representation();
                    let dup = self.heap.dup(&rep)?;
                    self.wake_fiber_with_value(waiter.fiber, dst, dup)
                }
                FiberOutcome::Failure(c) => {
                    let dup = self.heap.dup_cause(c)?;
                    self.wake_fiber_with_failure(waiter.fiber, dup)
                }
                FiberOutcome::Running => Ok(()),
            },
            FiberWaitKind::Await(dst) => {
                let exit = self.fiber_outcome_to_exit(child_id)?;
                self.wake_fiber_with_value(waiter.fiber, dst, exit)
            }
            FiberWaitKind::CoordDone { .. } => unreachable!(),
        }
    }

    pub(super) fn fiber_outcome_to_exit(&mut self, fid: usize) -> Result<Value, Error> {
        match &self.fiber_table[&fid].outcome {
            FiberOutcome::Success(v) => {
                let rep = v.representation();
                let dup = self.heap.dup(&rep)?;
                Ok(self.heap.allocate(Data::Exit(ExitData::Success(dup))))
            }
            FiberOutcome::Failure(c) => {
                let dup = self.heap.dup_cause(c)?;
                Ok(self
                    .heap
                    .allocate(Data::Exit(ExitData::Failure(dup.into_value()))))
            }
            FiberOutcome::Running => Err(Error::ExpectedFiber),
        }
    }

    fn drop_fiber_outcome(&mut self, outcome: FiberOutcome) -> Result<(), Error> {
        match outcome {
            FiberOutcome::Running => Ok(()),
            FiberOutcome::Success(v) => self.heap.drop_value(v),
            FiberOutcome::Failure(c) => self.heap.drop_cause(c),
        }
    }

    pub(super) fn drain_fiber_ctx_roots(&mut self, mut ctx: FiberContext, roots: &mut Vec<Value>) {
        roots.extend(ctx.scratch);
        if let Some(reg) = ctx.pending.take() {
            Self::reg_into_roots(reg, roots);
        }
        if let Some(c) = ctx.failure.take() {
            roots.push(c.into_value());
        }
        if let Some(reg) = ctx.pending_handler.take() {
            Self::reg_into_roots(reg, roots);
        }
        if let Some(reg) = ctx.top.take() {
            Self::reg_into_roots(reg, roots);
        }
        roots.extend(ctx.arguments);
        roots.extend(ctx.inherited_services);
        for op in ctx.inherited_tail_ops {
            Self::reg_into_roots(op.function, roots);
        }
        roots.extend(ctx.fiber_refs.into_values());
        if let Some(res) = ctx.pending_provide_result.take() {
            match res {
                PendingProvideResult::Ready(_, v) => roots.push(v),
                PendingProvideResult::Failed(_, c) => roots.push(c.into_value()),
            }
        }
        if let Some((l, _, _)) = ctx.pending_layer_build.take() {
            roots.push(l);
        }
        for reg in ctx.registers.into_iter().flatten() {
            Self::reg_into_roots(reg, roots);
        }
        for frame in ctx.frames {
            self.detach_frame_vars(frame.vars, roots);
            self.drain_frame_iterators(frame.iterators, roots);
            self.drain_end_roots(frame.end, roots);
        }
        if let Some(d) = ctx.discard.take() {
            self.drain_discard_roots(d, roots);
        }
    }

    pub(super) fn cleanup_async_roots(&mut self, roots: &mut Vec<Value>) {
        roots.extend(mem::take(&mut self.inherited_services));
        for op in mem::take(&mut self.inherited_tail_ops) {
            Self::reg_into_roots(op.function, roots);
        }
        self.lazy_waiters.clear();
        roots.extend(mem::take(&mut self.fiber_refs).into_values());
        for c in mem::take(&mut self.root_defects) {
            roots.push(c.into_value());
        }
        if let Some(res) = self.pending_provide_result.take() {
            match res {
                PendingProvideResult::Ready(_, v) => roots.push(v),
                PendingProvideResult::Failed(_, c) => roots.push(c.into_value()),
            }
        }
        if let Some((l, _, _)) = self.pending_layer_build.take() {
            roots.push(l);
        }
        for state in mem::take(&mut self.closing_scopes).into_values() {
            self.drain_closing_scope_roots(*state, roots);
        }
        for pp in mem::take(&mut self.pending_provides).into_values() {
            roots.push(pp.body_thunk);
            roots.extend(pp.prev_memo);
        }
        for ctx in mem::take(&mut self.parked_fibers).into_values() {
            self.drain_fiber_ctx_roots(ctx, roots);
        }
        for rec in mem::take(&mut self.fiber_table).into_values() {
            match rec.outcome {
                FiberOutcome::Running => {}
                FiberOutcome::Success(v) => roots.push(v),
                FiberOutcome::Failure(c) => roots.push(c.into_value()),
            }
            if let Some(sup) = rec.supervisor {
                roots.push(sup.callback);
                roots.extend(sup.services);
            }
        }
        self.cleanup_coords_and_groups(roots);
        self.cleanup_memo_maps(roots);
        self.run_queue.clear();
        self.timers.clear();
        self.test_clock_waiter = None;
        self.isaac_rngs.clear();
        self.pending_resume_close = None;
    }
}
