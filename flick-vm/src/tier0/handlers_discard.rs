impl<'p> Machine<'p, '_> {
    /// Internal abort/discontinuation cannot be intercepted by ordinary Try.
    pub(super) fn discard_step(&mut self) -> Result<(), Error> {
        let frame = self.discard.as_mut().expect("active transfer").frames.pop();
        let Some(mut frame) = frame else {
            return self.finish_discard();
        };
        self.heap
            .drop_values(mem::take(&mut frame.locals).into_values_vec())?;
        let gen_frames = self.release_frame_resources(&mut frame)?;
        if !gen_frames.is_empty() {
            self.discard
                .as_mut()
                .expect("active transfer")
                .frames
                .extend(gen_frames);
        }
        self.discard_frame_destination(frame.destination)
    }

    fn finish_discard(&mut self) -> Result<(), Error> {
        let discard = self.discard.take().expect("completed transfer");
        if let Some(owner) = discard.owner {
            self.current_owner = owner;
        }
        match discard.next {
            TransferNext::Invoke(id) => {
                self.ready = Some(id);
            }
            TransferNext::Raise => {}
            TransferNext::LoopContinue {
                id,
                code,
                args,
                binding,
            } => {
                self.restore_evidence();
                for arg in &args {
                    match self.heap.dup(arg) {
                        Ok(dup) => self.scratch.push(dup),
                        Err(err) => {
                            self.heap.drop_values(args)?;
                            return Err(err);
                        }
                    }
                }
                self.call(
                    code,
                    Destination::Loop {
                        id,
                        code,
                        args,
                        binding,
                    },
                )?;
            }
            TransferNext::DeliverValue { binding, value } => {
                self.restore_evidence();
                self.store(binding, value)?;
            }
            TransferNext::ReturnNone { binding } | TransferNext::DeliverNone { binding } => {
                self.restore_evidence();
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store(binding, none)?;
            }
            TransferNext::ResumeRunning { saved_failure, .. } => {
                self.restore_evidence();
                if let Some(cause) = saved_failure {
                    if let Some(existing) = self.failure.take() {
                        self.heap.drop_cause(existing)?;
                    }
                    self.failure = Some(cause);
                    self.failure_elected = false;
                }
            }
            TransferNext::FinishGenerateStrict { binding, items } => {
                self.restore_evidence();
                let list = self.heap.construct("$list", items)?;
                self.store(binding, list)?;
            }
            TransferNext::FinishCutoff => {
                let mut frame = self.frames.pop().expect("cutoff cleanup frame");
                self.heap
                    .drop_values(mem::take(&mut frame.locals).into_values_vec())?;
                let gen_frames = self.release_frame_resources(&mut frame)?;
                self.restore_evidence();
                self.complete_cutoff_destination(frame.destination)?;
                self.queue_discard_frames(gen_frames);
            }
            TransferNext::Chain(next_discard) => {
                self.discard = Some(*next_discard);
            }
        }
        Ok(())
    }

    fn discard_frame_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        let mut roots = Vec::new();
        match destination {
            Destination::RecoverySelection { id } => self.abandon_recovery(id)?,
            Destination::RecoveryBody {
                owner, evidence, ..
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
            }
            Destination::LayerRun { provider, .. } => {
                let paused = self.discard.take().expect("active transfer");
                self.heap.drop_values(roots)?;
                return self
                    .leave_provider_outcome(provider, ScopeOutcome::Discard(Box::new(paused)));
            }
            Destination::Handle { activation, .. } => {
                let activation = self
                    .activations
                    .remove(&activation)
                    .expect("departed handler");
                self.current_owner = activation.owner;
                Self::activation_roots(activation, &mut roots);
            }
            Destination::Discontinue { cont_id, .. } => {
                if let Some(Some(captured)) = self.continuations.get_mut(&cont_id).map(Option::take)
                {
                    self.discard
                        .as_mut()
                        .expect("active transfer")
                        .frames
                        .extend(captured.frames);
                }
            }
            Destination::Invocation(id) => self.discard_invocation(id, &mut roots),
            Destination::Catch { handler, .. } | Destination::CatchDefect { handler, .. } => {
                roots.push(handler);
            }
            Destination::Initialize { name, .. } => {
                self.globals.remove(&name);
            }
            Destination::CloseScope {
                owner, previous, ..
            } => {
                if self.owner_needs_async_close(owner) {
                    self.heap.drop_values(roots)?;
                    let aborted = self.discard.as_ref().expect("active transfer").aborted;
                    let exit_override = if aborted {
                        Some(
                            self.heap
                                .cause(&[Reason::Fail(&Value::Unit)], &mut self.fuel)?,
                        )
                    } else {
                        None
                    };
                    let paused = self.discard.take().expect("active transfer");
                    return self.begin_close_scope_with_override(
                        owner,
                        previous,
                        ScopeOutcome::Discard(Box::new(paused)),
                        exit_override,
                        false,
                    );
                }
                self.close_mirrored_owner(owner)?;
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            Destination::ClosingScope { state } => {
                let owner = state.owner;
                let previous = state.previous;
                if state.masked {
                    self.mask_depth = state.saved_mask;
                }
                self.drain_closing_scope_roots(*state, &mut roots);
                self.close_mirrored_owner(owner)?;
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            Destination::RestoreOwner { previous, .. } => self.current_owner = previous,
            Destination::Loop { args: items, .. } | Destination::GenerateStrict { items, .. } => {
                roots.extend(items);
            }
            Destination::CliStep { state, .. } => state.drain_roots(&mut roots),
            Destination::GeneratorStep { gen_id, .. } => {
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = GeneratorRun::Done;
                }
            }
            Destination::LazyComplete { lazy, .. } => {
                self.reset_lazy_or_handoff(&lazy)?;
            }
            other => return self.discard_cleanup_destination(other),
        }
        self.heap.drop_values(roots)
    }

    fn discard_cleanup_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        let mut roots = Vec::new();
        match destination {
            Destination::AcquireRegister { release, .. } => roots.push(release),
            Destination::BracketUse {
                use_fn, release_fn, ..
            } => {
                roots.push(use_fn);
                roots.push(release_fn);
            }
            Destination::BracketRelease {
                resource,
                release_fn,
                ..
            } => return self.discard_bracket_release(resource, release_fn, roots),
            Destination::BracketFinish {
                outcome,
                saved_mask,
                ..
            }
            | Destination::EnsuringDone {
                outcome,
                saved_mask,
                ..
            }
            | Destination::OnExitRunFin {
                outcome,
                saved_mask,
                ..
            }
            | Destination::OnExitFinish {
                outcome,
                saved_mask,
                ..
            } => {
                self.mask_depth = saved_mask;
                if let Some(cause) = self.take_defect_outcome(outcome, &mut roots)? {
                    return self.escalate_discard_defect(cause, roots);
                }
            }
            Destination::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            }
            | Destination::OnExitDiscardRunFin {
                saved_mask,
                masked,
                paused,
            } => {
                if masked {
                    self.mask_depth = saved_mask;
                }
                self.drain_discard_roots(*paused, &mut roots);
            }
            Destination::RestoreMask {
                saved_mask, next, ..
            } => {
                self.mask_depth = saved_mask;
                self.heap.drop_values(roots)?;
                return self.discard_frame_destination(*next);
            }
            Destination::EnsuringFinish { finalizer, .. } => {
                return self.discard_ensuring_finish(finalizer, roots);
            }
            Destination::OnExitDecide {
                interruptible,
                decide,
                ..
            } => return self.discard_on_exit_decide(interruptible, decide, roots),
            other => return self.discard_env_destination(other),
        }
        self.heap.drop_values(roots)
    }

    fn discard_ensuring_finish(
        &mut self,
        finalizer: Value,
        roots: Vec<Value>,
    ) -> Result<(), Error> {
        let paused = self.discard.take().expect("active transfer");
        self.heap.drop_values(roots)?;
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        self.scratch.extend([finalizer, Value::Unit]);
        self.await_task_fresh(Destination::DiscardCleanupDone {
            saved_mask,
            masked: true,
            paused: Box::new(paused),
        })
    }

    fn discard_on_exit_decide(
        &mut self,
        interruptible: bool,
        decide: Value,
        roots: Vec<Value>,
    ) -> Result<(), Error> {
        let paused = self.discard.take().expect("active transfer");
        self.heap.drop_values(roots)?;
        let exit_val = if let Some(cause) = Self::peek_discard_exit_cause(&paused.next) {
            let dup = match self.heap.dup_cause(cause) {
                Ok(dup) => dup,
                Err(err) => {
                    self.discard = Some(paused);
                    self.scratch.push(decide);
                    return Err(err);
                }
            };
            self.heap
                .allocate(Data::Exit(super::ExitData::Failure(dup.into_value())))
        } else {
            self.heap
                .allocate(Data::Exit(super::ExitData::Success(Value::Unit)))
        };
        let saved_mask = self.mask_depth;
        if !interruptible {
            self.mask_depth += 1;
        }
        self.scratch.extend([decide, exit_val]);
        self.apply_fresh(Destination::OnExitDiscardRunFin {
            saved_mask,
            masked: !interruptible,
            paused: Box::new(paused),
        })
    }

    fn discard_env_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        let mut roots = Vec::new();
        match destination {
            Destination::CatchCause { handler, .. }
            | Destination::ProvideContext {
                context: handler, ..
            } => roots.push(handler),
            Destination::WithServices { services, next } => {
                roots.extend(services);
                self.heap.drop_values(roots)?;
                return self.discard_frame_destination(*next);
            }
            Destination::CachedComplete { next, .. } => {
                self.heap.drop_values(roots)?;
                return self.discard_frame_destination(*next);
            }
            Destination::RestoreFiberRef { key, previous, .. } => {
                let old = match previous {
                    Some(v) => self.fiber_refs.insert(key, v),
                    None => self.fiber_refs.remove(&key),
                };
                roots.extend(old);
            }
            Destination::RestoreOwnerEvidence {
                previous, evidence, ..
            } => {
                self.current_owner = previous;
                self.current_evidence = evidence;
            }
            Destination::RestoreEnvRefs { previous, .. } => {
                let old = mem::replace(&mut self.fiber_refs, previous);
                roots.extend(old.into_values());
            }
            Destination::RestoreRandom {
                previous, rng_id, ..
            } => {
                self.seeded_random = previous;
                self.isaac_rngs.remove(&rng_id);
            }
            Destination::LayerBuildFinish {
                inner_owner,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry,
                target,
                ..
            } => {
                return self.discard_layer_build_finish(
                    inner_owner,
                    prev_owner,
                    prev_ev,
                    prev_memo,
                    memo_entry,
                    target,
                    roots,
                );
            }
            Destination::ForeignLayerBuildFinish { target, .. } => {
                return self.discard_foreign_layer_build_finish(target, roots);
            }
            Destination::OrDie { .. }
            | Destination::TimedFinish { .. }
            | Destination::RecoveryHelper { .. }
            | Destination::CaptureExit { .. }
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Modify { .. }
            | Destination::FunctionBoundary { .. }
            | Destination::DiscardFinalizer => {}
            _ => unreachable!("handled in discard_frame_destination"),
        }
        self.heap.drop_values(roots)
    }

    fn take_defect_outcome(
        &self,
        outcome: BracketOutcome,
        roots: &mut Vec<Value>,
    ) -> Result<Option<super::CauseToken>, Error> {
        match outcome {
            BracketOutcome::Success(v) => {
                roots.push(v);
                Ok(None)
            }
            BracketOutcome::Failure(cause) => {
                let has_die = self
                    .heap
                    .cause_reasons(&cause)?
                    .any(|r| matches!(r, super::Reason::Die(_)));
                if has_die {
                    Ok(Some(cause))
                } else {
                    roots.push(cause.into_value());
                    Ok(None)
                }
            }
        }
    }

    fn escalate_discard_defect(
        &mut self,
        cause: super::CauseToken,
        mut roots: Vec<Value>,
    ) -> Result<(), Error> {
        if let Some(mut discard) = self.discard.take() {
            self.drain_transfer_next_roots(&mut discard.next, &mut roots);
            self.frames.extend(discard.frames);
            self.restore_evidence();
        }
        if let Some(prev) = self.failure.replace(cause) {
            roots.push(prev.into_value());
        }
        self.failure_elected = true;
        self.heap.drop_values(roots)
    }

    fn discard_bracket_release(
        &mut self,
        resource: Value,
        release_fn: Value,
        roots: Vec<Value>,
    ) -> Result<(), Error> {
        let paused = self.discard.take().expect("active transfer");
        self.heap.drop_values(roots)?;
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        self.scratch.extend([release_fn, resource]);
        self.apply_fresh(Destination::DiscardCleanupDone {
            saved_mask,
            masked: true,
            paused: Box::new(paused),
        })
    }

    fn discard_invocation(&mut self, id: usize, roots: &mut Vec<Value>) {
        let invocation = self.invocations.remove(&id).expect("departed invocation");
        roots.extend(invocation.callee);
        roots.extend(invocation.arguments);
        match invocation.end {
            InvocationEnd::Tail(id) => {
                let segment = self.segments.remove(&id).expect("departed tail segment");
                self.discard
                    .as_mut()
                    .expect("active transfer")
                    .frames
                    .extend(segment.frames);
            }
            InvocationEnd::Control { cont_id, .. } => {
                if let Some(Some(captured)) = self.continuations.get_mut(&cont_id).map(Option::take)
                {
                    self.discard
                        .as_mut()
                        .expect("active transfer")
                        .frames
                        .extend(captured.frames);
                }
            }
            InvocationEnd::Caller(_) | InvocationEnd::Abort(_) => {}
        }
    }
}
