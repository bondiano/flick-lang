enum RunPoll {
    Exit(LanguageExit),
    Continue,
    Step,
}

impl Machine<'_, '_> {
    fn finish(&mut self, result: Result<LanguageExit, Error>) -> ProgramOutcome {
        let outcome = match result.and_then(|exit| self.report_exit(exit)) {
            Ok(outcome) => outcome,
            Err(error) => ProgramOutcome::Fault(error),
        };
        if let Err(error) = self.cleanup() {
            let _ = outcome.release(self.heap);
            self.restore_heap_limit();
            return ProgramOutcome::Fault(error);
        }
        self.restore_heap_limit();
        outcome
    }

    fn report_exit(&mut self, exit: LanguageExit) -> Result<ProgramOutcome, Error> {
        match exit {
            LanguageExit::Success(value) => {
                self.scratch.push(value);
                self.heap
                    .admit_ordinary(self.scratch.last().expect("success root"))?;
                Ok(ProgramOutcome::Returned(
                    self.scratch.pop().expect("ordinary success"),
                ))
            }
            LanguageExit::Failure(cause) => {
                // Root before fallible reporting; only singleton ordinary Fail is canonical Raised.
                self.failure = Some(cause);
                if self.reportable_failure()? {
                    Ok(ProgramOutcome::Raised(self.take_failure_payload()?))
                } else {
                    Ok(ProgramOutcome::Failed(
                        self.failure.take().expect("failure root"),
                    ))
                }
            }
        }
    }

    fn combine_root_defects(
        &mut self,
        base: Option<CauseToken>,
    ) -> Result<Option<CauseToken>, Error> {
        let mut acc = base;
        while !self.root_defects.is_empty() {
            let defect = self.root_defects.remove(0);
            acc = Some(match acc {
                None => defect,
                Some(prev) => {
                    let res = primitives::join_causes(self.heap, &prev, &defect, &mut FuelCounter(&mut self.fuel));
                    self.heap.drop_cause(prev)?;
                    self.heap.drop_cause(defect)?;
                    res?
                }
            });
        }
        Ok(acc)
    }

    fn run(&mut self) -> Result<LanguageExit, Error> {
        loop {
            if self.drain_pending_transitions()? {
                continue;
            }
            self.poll_external_interrupt()?;
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            self.drain_dropped_continuations();
            match self.check_run_completion()? {
                RunPoll::Exit(exit) => return Ok(exit),
                RunPoll::Continue => continue,
                RunPoll::Step => {}
            }
            if self.check_sandbox_pre_step()? {
                continue;
            }
            if self.fuel == 0 {
                if let Some(abort) = self.sandbox.abort.take() {
                    return Err(abort.into_error());
                }
                if self.sandbox.fuel_limited {
                    self.begin_sandbox_unwind(SandboxAbort::Limit(LimitKind::Fuel))?;
                    continue;
                }
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            self.maybe_quantum_preempt()?;
            if self.pending_resume_close.is_some() || self.pending_provide_result.is_some() {
                continue;
            }
            self.dispatch_run_step()?;
        }
    }

    fn drain_pending_transitions(&mut self) -> Result<bool, Error> {
        if let Some(owner) = self.pending_resume_close.take() {
            self.resume_owner_close(owner)?;
            return Ok(true);
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
            return Ok(true);
        }
        if let Some((layer, memo_id, sub_owner)) = self.pending_layer_build.take() {
            self.begin_build_layer(layer, memo_id, sub_owner, false, LayerBuildTarget::Top)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn check_run_completion(&mut self) -> Result<RunPoll, Error> {
        if self.discard.is_none()
            && self.ready.is_none()
            && self.failure.is_some()
            && self.frames.is_empty()
        {
            let cause = self.failure.take().expect("failure root");
            if self.active_fiber != 1 {
                self.complete_child_fiber(FiberOutcome::Failure(cause))?;
                return Ok(RunPoll::Continue);
            }
            if let Some(abort) = self.sandbox.abort.take() {
                self.heap.drop_cause(cause)?;
                return Err(abort.into_error());
            }
            let final_cause = self
                .combine_root_defects(Some(cause))?
                .expect("root failure");
            return Ok(RunPoll::Exit(LanguageExit::Failure(final_cause)));
        }
        if self.discard.is_none()
            && self.ready.is_none()
            && self.frames.is_empty()
            && self.arguments.is_empty()
            && self.top.is_some()
        {
            if self.active_fiber != 1 {
                let val = self.top.take().expect("child fiber return");
                self.complete_child_fiber(FiberOutcome::Success(val))?;
                return Ok(RunPoll::Continue);
            }
            if self.main_stage != MainStage::Done {
                let res = self.advance_main();
                self.handle_sandbox_step_result(res)?;
                return Ok(RunPoll::Continue);
            }
            let val = self.top.take().expect("completed top outcome");
            if !self.root_defects.is_empty() {
                self.heap.drop_value(val)?;
                let cause = self.combine_root_defects(None)?.expect("root defects");
                return Ok(RunPoll::Exit(LanguageExit::Failure(cause)));
            }
            return Ok(RunPoll::Exit(LanguageExit::Success(val)));
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
        Ok(RunPoll::Step)
    }

    fn dispatch_run_step(&mut self) -> Result<(), Error> {
        if self.discard.is_some() {
            let res = self.discard_step();
            self.handle_sandbox_step_result(res)?;
        } else if let Some(id) = self.ready.take() {
            let res = self.invoke_step(id);
            self.handle_sandbox_step_result(res)?;
        } else if self.failure.is_some() {
            self.unwind()?;
        } else if let Some(value) = self.top.take() {
            self.scratch.push(value);
            self.scratch.push(
                self.arguments
                    .pop_front()
                    .expect("top application has an argument"),
            );
            let res = self.apply(Destination::Top);
            self.handle_sandbox_step_result(res)?;
        } else if !self.frames.is_empty() {
            let res = self.step();
            self.handle_sandbox_step_result(res)?;
        }
        Ok(())
    }

    fn poll_external_interrupt(&mut self) -> Result<(), Error> {
        if self
            .external_interrupt
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            self.interrupt_active_fiber()?;
        }
        Ok(())
    }

    fn interrupt_active_fiber(&mut self) -> Result<(), Error> {
        if self.interrupt_requested.is_none() {
            let target = self.active_fiber;
            self.request_interrupt(target, 0)?;
            if target != 1 {
                self.request_interrupt(1, 0)?;
            }
        }
        self.check_active_interrupt()?;
        Ok(())
    }

    fn step(&mut self) -> Result<(), Error> {
        let (block, at) = {
            let frame = self.frames.last().expect("a running code frame");
            (frame.block, frame.at)
        };
        if let Some(instruction) = block.instructions.get(at) {
            self.frames.last_mut().expect("running frame").at += 1;
            match instruction {
                Instruction::Let { binding, value } => {
                    if let Err(error) = self.operation(*binding, value) {
                        match StepFailure::classify(error) {
                            StepFailure::Language(spec) => self.raise_defect(spec)?,
                            StepFailure::Evaluator(error) => return Err(error),
                        }
                    }
                }
                Instruction::Dup(local) => {
                    let frame = self.frames.last_mut().expect("running frame");
                    let source = peek(&frame.locals, *local)?;
                    self.heap.admit_not_reuse_token(source)?;
                    let value = self.heap.dup(source)?;
                    frame.locals.push(*local, value);
                }
                Instruction::Drop(local) => {
                    let value = take(
                        &mut self.frames.last_mut().expect("running frame").locals,
                        *local,
                    )?;
                    self.scratch.push(value);
                    self.release_operands()?;
                }
            }
            return Ok(());
        }
        match &block.exit {
            Exit::Return(local) => {
                let frame = self.frames.last_mut().expect("returning frame");
                self.pending = Some(take(&mut frame.locals, *local)?);
                if matches!(
                    frame.destination,
                    Destination::RecoverySelection { .. } | Destination::RecoveryHelper { .. }
                ) {
                    self.heap
                        .admit_not_reuse_token(self.pending.as_ref().expect("return root"))?;
                } else {
                    self.heap
                        .admit_ordinary(self.pending.as_ref().expect("return root"))?;
                }
                if !frame.locals.is_empty() {
                    return Err(Error::Leftover(frame.locals.leftover()));
                }
                let mut frame = self.frames.pop().expect("returning frame");
                let gen_frames = self.release_frame_resources(&mut frame)?;
                self.restore_evidence();
                self.returned_frame(frame.destination)?;
                self.queue_discard_frames(gen_frames);
            }
            Exit::Branch {
                condition,
                then_,
                else_,
            } => {
                let frame = self.frames.last_mut().expect("branching frame");
                self.heap.admit_ordinary(peek(&frame.locals, *condition)?)?;
                let Value::Bool(condition_value) = peek(&frame.locals, *condition)? else {
                    return Err(Error::ExpectedBoolean);
                };
                let condition_value = *condition_value;
                let _ = take(&mut frame.locals, *condition)?;
                frame.block = if condition_value { then_ } else { else_ };
                frame.at = 0;
            }
        }
        Ok(())
    }
}
