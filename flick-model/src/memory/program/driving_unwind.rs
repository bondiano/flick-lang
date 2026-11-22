// Frame unwinding, interrupted-handler skipping, and lazy unwind/handoff.

impl<'p> Machine<'p, '_> {
    /// Each suspended-frame crossing consumes one ordinary VM step.
    fn unwind(&mut self) -> Result<(), Error> {
        let mut frame = self.frames.pop().expect("raised frame");
        self.restore_evidence();
        let destination = mem::replace(&mut frame.destination, Destination::DiscardFinalizer);
        self.heap
            .drop_values(mem::take(&mut frame.locals).into_values_vec())?;
        let gen_frames = self.release_frame_resources(&mut frame)?;
        self.fail_destination(destination)?;
        self.queue_discard_frames(gen_frames);
        Ok(())
    }

    fn skip_interrupted_handler(&mut self) -> Result<bool, Error> {
        let Some(by) = self.interrupt_requested else {
            return Ok(false);
        };
        if self.mask_depth > 0 {
            return Ok(false);
        }
        let prev = self.failure.take();
        let fid = usize::try_from(by).ok();
        let res =
            primitives::strip_fail_and_add_interrupt(self.heap, prev.as_ref(), fid, &mut self.fuel);
        if let Some(p) = prev {
            self.heap.drop_cause(p)?;
        }
        self.failure = Some(res?);
        self.failure_elected = true;
        Ok(true)
    }

    fn fail_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::Handle {
                activation,
                binding,
            } => self.handler_failed(activation, binding),
            Destination::Catch { handler, binding } => {
                if self.skip_interrupted_handler()? {
                    self.heap.drop_value(handler)?;
                    return Ok(());
                }
                self.pending_handler = Some(handler);
                self.begin_recovery(binding)
            }
            Destination::CaptureExit { binding } => {
                if self.skip_interrupted_handler()? {
                    return Ok(());
                }
                let cause = self.failure.take().expect("captured exit failure");
                self.failure_elected = false;
                let exit = self
                    .heap
                    .allocate(Data::Exit(ExitData::Failure(cause.into_value())));
                self.store(binding, exit)
            }
            Destination::CatchDefect { handler, binding } => {
                if self.skip_interrupted_handler()? {
                    self.heap.drop_value(handler)?;
                    return Ok(());
                }
                self.pending_handler = Some(handler);
                let defect_msg = self
                    .failure
                    .as_ref()
                    .map(|c| primitives::first_defect_message(self.heap, c.object_id()))
                    .transpose()?
                    .flatten();
                let handler = self.pending_handler.take().expect("defect handler");
                if let Some(msg) = defect_msg {
                    if let Some(cause) = self.failure.take() {
                        self.heap.drop_cause(cause)?;
                    }
                    self.failure_elected = false;
                    let msg_val = self.heap.literal(&Lit::String(msg))?;
                    self.scratch.extend([handler, msg_val]);
                    self.apply(Destination::Bind(binding))
                } else {
                    self.heap.drop_value(handler)
                }
            }
            other => self.unwind_destination(other),
        }
    }

    fn unwind_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::RecoverySelection { id } => self.abandon_recovery(id)?,
            Destination::RecoveryBody {
                owner, evidence, ..
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
            }
            Destination::LayerRun { provider, .. } => {
                let cause = self.failure.take().expect("provider failure cause");
                self.leave_provider_outcome(provider, ScopeOutcome::Failure(cause))?;
            }
            Destination::Discontinue { cont_id, .. } => self.unwind_discontinue(cont_id),
            Destination::Invocation(id) => self.invocation_failed(id)?,
            Destination::Initialize { name, .. } => {
                self.globals.remove(&name);
            }
            Destination::CloseScope {
                owner, previous, ..
            } => {
                let cause = self.failure.take().expect("scope failure cause");
                self.begin_close_scope(owner, previous, ScopeOutcome::Failure(cause))?;
            }
            Destination::ClosingScope { state } => self.finalizer_failed(state)?,
            Destination::RestoreOwner { previous, .. } => self.current_owner = previous,
            Destination::Loop { args, .. } => self.heap.drop_values(args)?,
            Destination::GenerateStrict { items, .. } => self.heap.drop_values(items)?,
            Destination::GeneratorStep { gen_id, .. } => {
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = GeneratorRun::Done;
                }
            }
            Destination::LazyComplete { lazy, .. } => self.unwind_lazy(&lazy)?,
            Destination::AcquireRegister { release, .. } => self.heap.drop_value(release)?,
            Destination::BracketUse {
                use_fn, release_fn, ..
            } => self.heap.drop_values(vec![use_fn, release_fn])?,
            Destination::BracketRelease {
                resource,
                release_fn,
                binding,
            } => self.unwind_bracket_release(resource, release_fn, binding)?,
            Destination::BracketFinish {
                outcome,
                saved_mask,
                ..
            } => self.finish_masked_cleanup_err(outcome, saved_mask)?,
            Destination::RestoreMask { .. }
            | Destination::DiscardCleanupDone { .. }
            | Destination::OnExitDiscardRunFin { .. }
            | Destination::EnsuringFinish { .. }
            | Destination::EnsuringDone { .. }
            | Destination::OnExitDecide { .. }
            | Destination::OnExitRunFin { .. }
            | Destination::OnExitFinish { .. }
            | Destination::CatchCause { .. }
            | Destination::OrDie { .. }
            | Destination::TimedFinish { .. }
            | Destination::CachedComplete { .. }
            | Destination::ProvideContext { .. }
            | Destination::WithServices { .. }
            | Destination::RestoreFiberRef { .. }
            | Destination::RestoreOwnerEvidence { .. }
            | Destination::RestoreEnvRefs { .. }
            | Destination::RestoreRandom { .. }
            | Destination::LayerBuildFinish { .. } => {
                self.unwind_async_destination(destination)?;
            }
            Destination::RecoveryHelper { .. }
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Modify { .. }
            | Destination::FunctionBoundary { .. }
            | Destination::DiscardFinalizer => {}
            Destination::Catch { .. }
            | Destination::CaptureExit { .. }
            | Destination::CatchDefect { .. }
            | Destination::Handle { .. } => unreachable!("handled in fail_destination"),
        }
        Ok(())
    }

    fn unwind_discontinue(&mut self, cont_id: usize) {
        if let Some(Some(captured)) = self.continuations.get_mut(&cont_id).map(Option::take) {
            let cause = self.failure.take();
            self.discard = Some(Discard {
                frames: captured.frames,
                owner: Some(self.current_owner),
                next: TransferNext::ResumeRunning {
                    saved_failure: cause,
                },
                aborted: true,
            });
        }
    }

    fn unwind_bracket_release(
        &mut self,
        resource: Value,
        release_fn: Value,
        binding: Local,
    ) -> Result<(), Error> {
        let cause = self.failure.take().expect("bracket body failure");
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        self.scratch.extend([release_fn, resource]);
        self.apply_fresh(Destination::BracketFinish {
            outcome: BracketOutcome::Failure(cause),
            saved_mask,
            binding,
        })
    }

    fn unwind_lazy(&mut self, lazy: &LazyHandle) -> Result<(), Error> {
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
            if let Some(LazyState::Running { task, .. }) = prev {
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
                let (wfib, wbind) = waiters.remove(0);
                let still_parked = self.fiber_table.get(&wfib).is_some_and(|r| {
                    matches!(r.parked, Some(ParkReason::Lazy { lazy_id }) if lazy_id == lazy.id)
                });
                if still_parked {
                    next_waiter = Some((wfib, wbind));
                    break;
                }
            }
            if waiters.is_empty() {
                self.lazy_waiters.remove(&lazy.id);
            }
        }
        if let Some((wfib, wbind)) = next_waiter {
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
                self.scratch.extend([task_dup, Value::Unit]);
                let res = self.await_task_fresh(Destination::LazyComplete {
                    lazy: handle,
                    binding: wbind,
                });
                self.switch_to_fiber(caller);
                res?;
                self.run_queue.push_back(wfib);
                return Ok(());
            }
        }
        let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
        if let Some(LazyState::Running { task, .. }) = slot.take() {
            *slot = Some(LazyState::Pending(task));
        }
        Ok(())
    }
}
