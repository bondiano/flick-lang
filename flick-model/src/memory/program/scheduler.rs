// Cooperative scheduler loop, timers, TestClock stepping, child completion, and async cleanup.

impl<'p> Machine<'p, '_> {
    fn schedule_next(&mut self, active_finished: bool) -> Result<(), Error> {
        if self.foreign.sync_task_fiber.is_some() {
            return Ok(());
        }
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
            if self.try_poll_bridge_nonblocking()? {
                continue;
            }
            if self.test_clock_mode {
                if self.advance_test_clock_step()? {
                    continue;
                }
            } else if self.has_pending_foreign_work() && !self.timers.is_empty() {
                if self.poll_bridge_with_timer_budget()? {
                    continue;
                }
            } else if self.fire_next_timer()? {
                continue;
            }
            if self.has_pending_foreign_work() {
                self.poll_bridge_once()?;
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

    fn maybe_quantum_preempt(&mut self) -> Result<(), Error> {
        if self.foreign.sync_task_fiber.is_some() {
            return Ok(());
        }
        if self.interrupt_requested.is_some()
            && !self.frames.iter().any(|frame| {
                matches!(
                    frame.destination,
                    Destination::Modify { .. } | Destination::Initialize { .. }
                )
            })
            && self.check_active_interrupt()?
        {
            return Ok(());
        }
        if self.quantum > 0 {
            self.quantum -= 1;
            return Ok(());
        }
        self.quantum = self.active_max_ops_before_yield();
        if self.frames.iter().any(|frame| {
            matches!(
                frame.destination,
                Destination::Modify { .. } | Destination::Initialize { .. }
            )
        }) {
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

    fn register_timer(&mut self, delay_ns: u64, action: TimerAction) -> usize {
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

    fn cancel_timer(&mut self, timer_id: usize) {
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
            TimerAction::WakeSleep { fiber, binding } => {
                let matches = self.fiber_table.get(&fiber).is_some_and(|r| {
                    matches!(r.parked, Some(ParkReason::Sleep { timer_id }) if timer_id == entry.id)
                });
                if matches {
                    self.wake_fiber_with_value(fiber, binding, Value::Unit)?;
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
        self.wake_fiber_with_value(adv.caller_fiber, adv.binding, Value::Unit)?;
        Ok(!self.run_queue.is_empty())
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
        if let Some(req_id) = self.foreign.pending_host_cbs.remove(&fid) {
            self.send_async_cb_return(req_id, fid)?;
        }
        Ok(())
    }

    fn complete_child_fiber(&mut self, outcome: FiberOutcome) -> Result<(), Error> {
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
            FiberWaitKind::Interrupt(binding) => {
                self.wake_fiber_with_value(waiter.fiber, binding, Value::Unit)
            }
            FiberWaitKind::Join(binding) => match &self.fiber_table[&child_id].outcome {
                FiberOutcome::Success(v) => {
                    let rep = v.representation();
                    let dup = self.heap.dup(&rep)?;
                    self.wake_fiber_with_value(waiter.fiber, binding, dup)
                }
                FiberOutcome::Failure(c) => {
                    let dup = self.heap.dup_cause(c)?;
                    self.wake_fiber_with_failure(waiter.fiber, dup)
                }
                FiberOutcome::Running => Ok(()),
            },
            FiberWaitKind::Await(binding) => {
                let exit = self.fiber_outcome_to_exit(child_id)?;
                self.wake_fiber_with_value(waiter.fiber, binding, exit)
            }
            FiberWaitKind::CoordDone { .. } => unreachable!(),
        }
    }

    fn fiber_outcome_to_exit(&mut self, fid: usize) -> Result<Value, Error> {
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

    fn drain_fiber_ctx_roots(&mut self, mut ctx: FiberContext<'p>, roots: &mut Vec<Value>) {
        roots.extend(ctx.scratch);
        roots.extend(ctx.pending);
        if let Some(c) = ctx.failure {
            roots.push(c.into_value());
        }
        roots.extend(ctx.pending_handler);
        roots.extend(ctx.top);
        roots.extend(ctx.arguments);
        roots.extend(ctx.inherited_services);
        roots.extend(ctx.inherited_tail_ops.into_iter().map(|o| o.function));
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
        for frame in ctx.frames {
            self.drain_frame_roots(frame, roots);
        }
        if let Some(d) = ctx.discard.take() {
            self.drain_discard_roots(d, roots);
        }
    }

    fn cleanup_async_roots(&mut self, roots: &mut Vec<Value>) {
        roots.extend(mem::take(&mut self.inherited_services));
        roots.extend(
            mem::take(&mut self.inherited_tail_ops)
                .into_iter()
                .map(|o| o.function),
        );
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
