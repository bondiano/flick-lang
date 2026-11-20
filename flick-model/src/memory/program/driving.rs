impl<'p> Machine<'p, '_> {
    fn finish(&mut self, result: Result<LanguageExit, Error>) -> ProgramOutcome {
        let outcome = match result.and_then(|exit| self.report_exit(exit)) {
            Ok(outcome) => outcome,
            Err(error) => ProgramOutcome::Fault(error),
        };
        if let Err(error) = self.cleanup() {
            let _ = outcome.release(self.heap);
            return ProgramOutcome::Fault(error);
        }
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

    fn run(&mut self) -> Result<LanguageExit, Error> {
        loop {
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            self.drain_dropped_continuations();
            if self.discard.is_none()
                && self.ready.is_none()
                && self.failure.is_some()
                && self.frames.is_empty()
            {
                return Ok(LanguageExit::Failure(
                    self.failure.take().expect("failure root"),
                ));
            }
            if self.discard.is_none()
                && self.ready.is_none()
                && self.frames.is_empty()
                && self.arguments.is_empty()
                && self.top.is_some()
            {
                if self.main_stage != MainStage::Done {
                    self.advance_main()?;
                    continue;
                }
                return Ok(LanguageExit::Success(
                    self.top.take().expect("completed top outcome"),
                ));
            }
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            if self.discard.is_some() {
                self.discard_step()?;
            } else if let Some(id) = self.ready.take() {
                self.invoke_step(id)?;
            } else if self.failure.is_some() {
                self.unwind()?;
            } else if let Some(value) = self.top.take() {
                self.scratch.push(value);
                self.scratch.push(
                    self.arguments
                        .pop_front()
                        .expect("top application has an argument"),
                );
                self.apply(Destination::Top)?;
            } else {
                self.step()?;
            }
        }
    }

    /// Each suspended-frame crossing consumes one ordinary VM step.
    fn unwind(&mut self) -> Result<(), Error> {
        let mut frame = self.frames.pop().expect("raised frame");
        self.restore_evidence();
        let destination = mem::replace(&mut frame.destination, Destination::DiscardFinalizer);
        if let Destination::Handle {
            activation,
            binding,
        } = destination
        {
            self.heap
                .drop_values(mem::take(&mut frame.locals).into_values_vec())?;
            let gen_frames = self.release_frame_resources(&mut frame)?;
            self.handler_failed(activation, binding)?;
            self.queue_discard_frames(gen_frames);
            return Ok(());
        }
        if let Destination::Catch { handler, binding } = destination {
            self.pending_handler = Some(handler);
            self.heap
                .drop_values(mem::take(&mut frame.locals).into_values_vec())?;
            let gen_frames = self.release_frame_resources(&mut frame)?;
            self.begin_recovery(binding)?;
            self.queue_discard_frames(gen_frames);
            return Ok(());
        }
        self.heap
            .drop_values(mem::take(&mut frame.locals).into_values_vec())?;
        let gen_frames = self.release_frame_resources(&mut frame)?;
        self.unwind_destination(destination)?;
        self.queue_discard_frames(gen_frames);
        Ok(())
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
            Destination::LayerRun { provider, .. } => self.leave_provider(provider)?,
            Destination::Discontinue { cont_id, .. } => {
                if let Some(Some(captured)) = self.continuations.get_mut(&cont_id).map(Option::take)
                {
                    let cause = self.failure.take();
                    self.discard = Some(Discard {
                        frames: captured.frames,
                        owner: Some(self.current_owner),
                        next: TransferNext::ResumeRunning {
                            saved_failure: cause,
                        },
                    });
                }
            }
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
            Destination::ClosingScope { mut state } => {
                let cause = self.failure.take().expect("finalizer failure cause");
                if state.finalizer_failure.is_none() {
                    state.finalizer_failure = Some(cause);
                } else {
                    self.heap.drop_cause(cause)?;
                }
                self.step_close_scope(state)?;
            }
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
            } => {
                let cause = self.failure.take().expect("bracket body failure");
                self.scratch.extend([release_fn, resource]);
                self.apply(Destination::BracketFinish {
                    outcome: BracketOutcome::Failure(cause),
                    binding,
                })?;
            }
            Destination::BracketFinish { outcome, .. } => match outcome {
                BracketOutcome::Success(value) => self.heap.drop_value(value)?,
                BracketOutcome::Failure(orig) => {
                    if let Some(secondary) = self.failure.take() {
                        self.heap.drop_cause(secondary)?;
                    }
                    self.failure = Some(orig);
                    self.failure_elected = false;
                }
            },
            Destination::RecoveryHelper { .. }
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Modify { .. }
            | Destination::FunctionBoundary { .. }
            | Destination::DiscardFinalizer => {}
            Destination::Catch { .. } => unreachable!("catch handled above"),
            Destination::Handle { .. } => unreachable!("handler handled above"),
        }
        Ok(())
    }

    fn unwind_lazy(&mut self, lazy: &LazyHandle) -> Result<(), Error> {
        let ordinary_payload = if let Some(cause) = self.failure.as_ref() {
            self.heap
                .ordinary_failure(cause)?
                .map(Value::representation)
        } else {
            None
        };
        if let Some(payload_rep) = ordinary_payload {
            let cached = self.heap.dup(&payload_rep)?;
            let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
            let prev = slot.replace(LazyState::DoneErr(cached));
            if let Some(LazyState::Running(task)) = prev {
                self.heap.drop_value(task)?;
            }
        } else {
            let slot = self.heap.lazy_state_mut(self.execution, lazy)?;
            if let Some(LazyState::Running(task)) = slot.take() {
                *slot = Some(LazyState::Pending(task));
            }
        }
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
                    let value = self.heap.dup(peek(&frame.locals, *local)?)?;
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
                if !matches!(
                    frame.destination,
                    Destination::RecoverySelection { .. } | Destination::RecoveryHelper { .. }
                ) {
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
