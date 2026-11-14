impl Machine<'_, '_> {
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
        let frame = self.frames.pop().expect("raised frame");
        self.restore_evidence();
        if let Destination::Handle {
            activation,
            binding,
        } = frame.destination
        {
            self.heap
                .drop_values(frame.locals.into_values().flatten().collect())?;
            return self.handler_failed(activation, binding);
        }
        if let Destination::Catch { handler, binding } = frame.destination {
            self.pending_handler = Some(handler);
            self.heap
                .drop_values(frame.locals.into_values().flatten().collect())?;
            self.begin_recovery(binding)?;
            return Ok(());
        }
        self.heap
            .drop_values(frame.locals.into_values().flatten().collect())?;
        match frame.destination {
            Destination::RecoverySelection { id } => self.abandon_recovery(id)?,
            Destination::RecoveryBody {
                owner, evidence, ..
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
            }
            Destination::LayerRun { provider, .. } => self.leave_provider(provider)?,
            Destination::Invocation(id) => self.invocation_failed(id)?,
            Destination::Initialize { name, .. } => {
                self.globals.remove(&name);
            }
            Destination::CloseScope {
                owner, previous, ..
            } => {
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            Destination::RestoreOwner { previous, .. } => self.current_owner = previous,
            Destination::RecoveryHelper { .. }
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Modify { .. } => {}
            Destination::Catch { .. } => unreachable!("catch handled above"),
            Destination::Handle { .. } => unreachable!("handler handled above"),
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
                    frame.locals.entry(*local).or_default().push(value);
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
                    return Err(Error::Leftover(
                        frame
                            .locals
                            .iter()
                            .map(|(id, values)| (*id, values.len()))
                            .collect(),
                    ));
                }
                let frame = self.frames.pop().expect("returning frame");
                self.restore_evidence();
                self.returned_frame(frame.destination)?;
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

    fn returned_frame(&mut self, destination: Destination) -> Result<(), Error> {
        match destination {
            Destination::RecoverySelection { id } => self.complete_recovery(id)?,
            Destination::RecoveryHelper { id, binding } => {
                let value = self.pending.take().expect("helper result");
                self.store_transport(binding, value, id)?;
            }
            Destination::RecoveryBody {
                binding,
                owner,
                evidence,
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
                let value = self.pending.take().expect("selected body result");
                self.store(binding, value)?;
            }
            Destination::LayerRun { provider, binding } => {
                self.leave_provider(provider)?;
                let value = self.pending.take().expect("layer result");
                self.store(binding, value)?;
            }
            Destination::Handle {
                activation,
                binding,
            } => self.handler_returned(activation, binding)?,
            Destination::Invocation(id) => {
                self.invocations
                    .get_mut(&id)
                    .expect("running invocation")
                    .callee = self.pending.take();
                self.ready = Some(id);
            }
            Destination::Catch { handler, binding } => {
                self.pending_handler = Some(handler);
                self.heap
                    .drop_value(self.pending_handler.take().expect("unused handler"))?;
                let value = self.pending.take().expect("try result");
                self.store(binding, value)?;
            }
            Destination::Initialize { name, binding } => {
                let value = self.pending.take().expect("returned token");
                self.globals.insert(name.clone(), Some(value));
                let root = self.globals[&name].as_ref().expect("initialized global");
                let owned = self.heap.dup(root)?;
                self.deliver(&binding.map_or(Destination::Top, Destination::Bind), owned)?;
            }
            Destination::Modify { cell, binding } => {
                self.complete_modify(&cell, binding)?;
            }
            Destination::CloseScope {
                owner,
                previous,
                binding,
            } => {
                self.close_owner(owner)?;
                self.current_owner = previous;
                let value = self.pending.take().expect("scope result");
                self.store(binding, value)?;
            }
            Destination::RestoreOwner { previous, binding } => {
                self.current_owner = previous;
                let value = self.pending.take().expect("selected-owner result");
                self.store(binding, value)?;
            }
            destination => {
                let value = self.pending.take().expect("returned token");
                self.deliver(&destination, value)?;
            }
        }
        Ok(())
    }
}
