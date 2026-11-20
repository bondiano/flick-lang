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
            TransferNext::ResumeRunning { saved_failure } => {
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
            Destination::LayerRun { provider, .. } => self.leave_provider(provider)?,
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
            Destination::Catch { handler, .. } => roots.push(handler),
            Destination::Initialize { name, .. } => {
                self.globals.remove(&name);
            }
            Destination::CloseScope {
                owner, previous, ..
            } => {
                if !self.owners[owner].finalizers.is_empty() {
                    let paused = self.discard.take().expect("active transfer");
                    self.heap.drop_values(roots)?;
                    return self.begin_close_scope(
                        owner,
                        previous,
                        ScopeOutcome::Discard(Box::new(paused)),
                    );
                }
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            Destination::ClosingScope { state } => {
                let owner = state.owner;
                let previous = state.previous;
                self.drain_closing_scope_roots(*state, &mut roots);
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            Destination::RestoreOwner { previous, .. } => self.current_owner = previous,
            Destination::Loop { args, .. } => roots.extend(args),
            Destination::GenerateStrict { items, .. } => roots.extend(items),
            Destination::GeneratorStep { gen_id, .. } => {
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = GeneratorRun::Done;
                }
            }
            Destination::LazyComplete { lazy, .. } => {
                let slot = self.heap.lazy_state_mut(self.execution, &lazy)?;
                if let Some(LazyState::Running(task)) = slot.take() {
                    *slot = Some(LazyState::Pending(task));
                }
            }
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
            Destination::BracketFinish { outcome, .. } => match outcome {
                BracketOutcome::Success(v) => roots.push(v),
                BracketOutcome::Failure(cause) => roots.push(cause.into_value()),
            },
            Destination::RecoveryHelper { .. }
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Modify { .. }
            | Destination::FunctionBoundary { .. }
            | Destination::DiscardFinalizer => {}
        }
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
        self.scratch.extend([release_fn, resource]);
        let state = Box::new(super::ClosingScopeState {
            owner: self.current_owner,
            previous: self.current_owner,
            remaining: Vec::new(),
            outcome: ScopeOutcome::Discard(Box::new(paused)),
            finalizer_failure: None,
        });
        self.apply(Destination::ClosingScope { state })
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
