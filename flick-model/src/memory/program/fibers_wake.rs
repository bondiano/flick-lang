// Cooperative fiber park, yield, interrupt, and wake operations.

impl Machine<'_, '_> {
    fn park_current(&mut self, reason: ParkReason) -> Result<(), Error> {
        let fid = self.active_fiber;
        if let Some(rec) = self.fiber_table.get_mut(&fid) {
            rec.parked = Some(reason);
        }
        self.schedule_next(false)
    }

    fn yield_current(&mut self, front_child: Option<usize>) -> Result<(), Error> {
        let fid = self.active_fiber;
        self.run_queue.push_back(fid);
        if let Some(child) = front_child {
            self.run_queue.retain(|&id| id != child);
            self.run_queue.push_front(child);
        }
        self.schedule_next(false)
    }

    fn check_active_interrupt(&mut self) -> Result<bool, Error> {
        if self.failure.is_some() || self.discard.is_some() || self.mask_depth > 0 {
            return Ok(false);
        }
        let Some(by) = self.interrupt_requested else {
            return Ok(false);
        };
        let cause = self.interrupt_cause(by)?;
        self.failure = Some(cause);
        self.failure_elected = true;
        Ok(true)
    }

    fn interrupt_cause(&mut self, by: u64) -> Result<CauseToken, Error> {
        let fid = usize::try_from(by).ok();
        self.heap
            .cause(&[Reason::Interrupt { fiber_id: fid }], &mut self.fuel)
    }

    fn request_interrupt(&mut self, target: usize, by: u64) -> Result<(), Error> {
        let Some(rec) = self.fiber_table.get(&target) else {
            return Ok(());
        };
        if !matches!(rec.outcome, FiberOutcome::Running) {
            return Ok(());
        }
        let masked = if target == self.active_fiber {
            self.interrupt_requested.get_or_insert(by);
            self.mask_depth > 0
        } else if let Some(ctx) = self.parked_fibers.get_mut(&target) {
            ctx.interrupt_requested.get_or_insert(by);
            ctx.mask_depth > 0
        } else {
            false
        };
        if masked {
            return Ok(());
        }
        let parked = self.fiber_table.get(&target).and_then(|r| r.parked);
        match parked {
            Some(ParkReason::Coord { coord_id, .. }) => {
                self.interrupt_coord_parent(coord_id, target, by)?;
            }
            Some(
                ParkReason::OwnerChildren { .. }
                | ParkReason::OwnerHook { .. }
                | ParkReason::TestClock,
            )
            | None => {}
            Some(ParkReason::Sleep { timer_id }) => {
                self.cancel_timer(timer_id);
                let cause = self.interrupt_cause(by)?;
                self.wake_fiber_with_failure(target, cause)?;
            }
            Some(ParkReason::MemoWait {
                target: build_target,
            }) => {
                let cause = self.interrupt_cause(by)?;
                self.wake_fiber_build_failure(target, build_target, cause)?;
            }
            Some(ParkReason::Lazy { lazy_id }) => {
                if let Some(waiters) = self.lazy_waiters.get_mut(&lazy_id) {
                    waiters.retain(|(fid, _)| *fid != target);
                }
                let cause = self.interrupt_cause(by)?;
                self.wake_fiber_with_failure(target, cause)?;
            }
            Some(
                ParkReason::Deferred { .. }
                | ParkReason::Fiber { .. }
                | ParkReason::Never
                | ParkReason::FiberGroupJoin
                | ParkReason::FiberGroupEmpty,
            ) => {
                let cause = self.interrupt_cause(by)?;
                self.wake_fiber_with_failure(target, cause)?;
            }
        }
        Ok(())
    }

    fn wake_fiber_with_value(
        &mut self,
        target: usize,
        binding: Local,
        val: Value,
    ) -> Result<(), Error> {
        let Some(rec) = self.fiber_table.get_mut(&target) else {
            return self.heap.drop_value(val);
        };
        if rec.parked.take().is_none() {
            return self.heap.drop_value(val);
        }
        if target == self.active_fiber {
            self.run_queue.push_back(target);
            return self.store(binding, val);
        }
        if let Some(ctx) = self.parked_fibers.get_mut(&target) {
            if let Some(frame) = ctx.frames.last_mut() {
                frame.locals.insert_one(binding, val);
            } else {
                ctx.top = Some(val);
            }
            self.run_queue.push_back(target);
            Ok(())
        } else {
            self.heap.drop_value(val)
        }
    }

    fn wake_fiber_build_value(
        &mut self,
        fiber: usize,
        target: LayerBuildTarget,
        val: Value,
    ) -> Result<(), Error> {
        match target {
            LayerBuildTarget::Local(binding) => self.wake_fiber_with_value(fiber, binding, val),
            LayerBuildTarget::Top => {
                let Some(rec) = self.fiber_table.get_mut(&fiber) else {
                    return self.heap.drop_value(val);
                };
                if rec.parked.take().is_none() {
                    return self.heap.drop_value(val);
                }
                if fiber == self.active_fiber {
                    self.top = Some(val);
                    self.run_queue.push_back(fiber);
                    return Ok(());
                }
                if let Some(ctx) = self.parked_fibers.get_mut(&fiber) {
                    ctx.top = Some(val);
                    self.run_queue.push_back(fiber);
                    Ok(())
                } else {
                    self.heap.drop_value(val)
                }
            }
            LayerBuildTarget::Provide(pid) => {
                let Some(rec) = self.fiber_table.get_mut(&fiber) else {
                    return self.heap.drop_value(val);
                };
                if rec.parked.take().is_none() {
                    return self.heap.drop_value(val);
                }
                if fiber == self.active_fiber {
                    self.pending_provide_result = Some(PendingProvideResult::Ready(pid, val));
                    self.run_queue.push_back(fiber);
                    return Ok(());
                }
                if let Some(ctx) = self.parked_fibers.get_mut(&fiber) {
                    ctx.pending_provide_result = Some(PendingProvideResult::Ready(pid, val));
                    self.run_queue.push_back(fiber);
                    Ok(())
                } else {
                    self.heap.drop_value(val)
                }
            }
        }
    }

    fn wake_fiber_build_failure(
        &mut self,
        fiber: usize,
        target: LayerBuildTarget,
        cause: CauseToken,
    ) -> Result<(), Error> {
        match target {
            LayerBuildTarget::Local(_) | LayerBuildTarget::Top => {
                self.wake_fiber_with_failure(fiber, cause)
            }
            LayerBuildTarget::Provide(pid) => {
                let Some(rec) = self.fiber_table.get_mut(&fiber) else {
                    return self.heap.drop_cause(cause);
                };
                if rec.parked.take().is_none() {
                    return self.heap.drop_cause(cause);
                }
                if fiber == self.active_fiber {
                    self.pending_provide_result = Some(PendingProvideResult::Failed(pid, cause));
                    self.run_queue.push_back(fiber);
                    return Ok(());
                }
                if let Some(ctx) = self.parked_fibers.get_mut(&fiber) {
                    ctx.pending_provide_result = Some(PendingProvideResult::Failed(pid, cause));
                    self.run_queue.push_back(fiber);
                    Ok(())
                } else {
                    self.heap.drop_cause(cause)
                }
            }
        }
    }

    fn wake_fiber_with_failure(&mut self, target: usize, cause: CauseToken) -> Result<(), Error> {
        let Some(rec) = self.fiber_table.get_mut(&target) else {
            return self.heap.drop_cause(cause);
        };
        if rec.parked.take().is_none() {
            return self.heap.drop_cause(cause);
        }
        if target == self.active_fiber {
            if let Some(prev) = self.failure.replace(cause) {
                self.heap.drop_cause(prev)?;
            }
            self.failure_elected = true;
            self.run_queue.push_back(target);
            return Ok(());
        }
        if let Some(ctx) = self.parked_fibers.get_mut(&target) {
            if let Some(prev) = ctx.failure.replace(cause) {
                self.heap.drop_cause(prev)?;
            }
            ctx.failure_elected = true;
            self.run_queue.push_back(target);
            Ok(())
        } else {
            self.heap.drop_cause(cause)
        }
    }

    fn wake_owner_closer(&mut self, owner: usize) {
        let Some(closer) = self.owners[owner].close_waiter.take() else {
            return;
        };
        if let Some(rec) = self.fiber_table.get_mut(&closer)
            && matches!(
                rec.parked,
                Some(ParkReason::OwnerChildren { owner: o } | ParkReason::OwnerHook { owner: o })
                if o == owner
            )
        {
            rec.parked = None;
            if closer == self.active_fiber {
                self.pending_resume_close = Some(owner);
                self.run_queue.push_back(closer);
            } else if let Some(ctx) = self.parked_fibers.get_mut(&closer) {
                ctx.pending_resume_close = Some(owner);
                self.run_queue.push_back(closer);
            }
        }
    }
}
