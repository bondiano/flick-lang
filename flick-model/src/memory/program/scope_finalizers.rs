// Lazy cells, LIFO Scope/Resource finalizers, and frame-resource cleanup/draining.
impl<'p> Machine<'p, '_> {
    fn control_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<bool, Error> {
        match kind {
            Intrinsic::LazyMake => {
                let task = self.scratch.pop().expect("lazy task");
                let root = self.heap.allocate(Data::Lazy {
                    execution: self.execution,
                    state: Some(LazyState::Pending(task)),
                });
                let id = root.object_id().expect("lazy allocation");
                self.lazies.push(root);
                self.store(
                    binding,
                    Value::Lazy(LazyHandle {
                        id,
                        execution: self.execution,
                    }),
                )?;
            }
            Intrinsic::LazyForce => self.lazy_force(binding)?,
            Intrinsic::ScopeAddFinalizer => {
                let task = self.scratch.pop().expect("finalizer task");
                self.register_scope_finalizer(binding, Finalizer::Task(task))?;
            }
            Intrinsic::ScopeAddFinalizerExit => {
                let func = self.scratch.pop().expect("exit finalizer");
                self.register_scope_finalizer(binding, Finalizer::WithCause(func))?;
            }
            Intrinsic::ScopeAddClose => {
                let func = self.scratch.pop().expect("close callback");
                self.register_scope_finalizer(binding, Finalizer::WithExit(func))?;
            }
            Intrinsic::AcquireRelease => {
                let release = self.scratch.pop().expect("release fn");
                let acquire = self.scratch.pop().expect("acquire task");
                let owner = self.current_owner;
                if self.owners[owner].closed || self.owners[owner].closing {
                    self.scratch.extend([acquire, release]);
                    self.raise_defect(DefectSpec::arena_owner_closed())?;
                    return Ok(true);
                }
                self.scratch.push(acquire);
                self.scratch.push(Value::Unit);
                self.await_task(Destination::AcquireRegister {
                    owner,
                    release,
                    binding,
                })?;
            }
            Intrinsic::Bracket => {
                let release_fn = self.scratch.pop().expect("bracket release");
                let use_fn = self.scratch.pop().expect("bracket use");
                let acquire = self.scratch.pop().expect("bracket acquire");
                self.scratch.push(acquire);
                self.scratch.push(Value::Unit);
                self.await_task(Destination::BracketUse {
                    use_fn,
                    release_fn,
                    binding,
                })?;
            }
            Intrinsic::Uninterruptible => {
                let task = self.scratch.pop().expect("masked task");
                self.scratch.push(task);
                self.scratch.push(Value::Unit);
                self.await_task(Destination::Bind(binding))?;
            }
            Intrinsic::SeqOfList => {
                let list = self.scratch.pop().expect("list for Seq.of_list");
                let value = self.heap.allocate(Data::Seq(SeqData::FromList(list)));
                self.store(binding, value)?;
            }
            Intrinsic::SeqToList => self.seq_to_list(binding)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn lazy_force(&mut self, binding: Local) -> Result<(), Error> {
        enum LazyAction {
            Return(Value),
            Raise(Value),
            Start(Value),
        }
        let Value::Lazy(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedLazy);
        };
        let handle = LazyHandle {
            id: handle.id,
            execution: handle.execution,
        };
        self.release_operands()?;
        let action = {
            let slot = self.heap.lazy_state_mut(self.execution, &handle)?;
            match slot {
                Some(LazyState::DoneOk(value)) => {
                    let rep = value.representation();
                    LazyAction::Return(self.heap.dup(&rep)?)
                }
                Some(LazyState::DoneErr(err)) => {
                    let rep = err.representation();
                    LazyAction::Raise(self.heap.dup(&rep)?)
                }
                Some(LazyState::Running(_)) => return Err(Error::LazyCycle),
                Some(LazyState::Pending(_)) => {
                    let Some(LazyState::Pending(task)) = slot.take() else {
                        unreachable!()
                    };
                    let rep = task.representation();
                    *slot = Some(LazyState::Running(task));
                    LazyAction::Start(self.heap.dup(&rep)?)
                }
                None => return Err(Error::ClosedLazy),
            }
        };
        match action {
            LazyAction::Return(value) => self.store(binding, value),
            LazyAction::Raise(err) => {
                self.scratch.push(err);
                self.raise_ordinary()
            }
            LazyAction::Start(task) => {
                self.scratch.push(task);
                self.scratch.push(Value::Unit);
                self.await_task(Destination::LazyComplete {
                    lazy: handle,
                    binding,
                })
            }
        }
    }

    fn register_scope_finalizer(
        &mut self,
        binding: Local,
        finalizer: Finalizer,
    ) -> Result<(), Error> {
        if self.owners[self.current_owner].closed || self.owners[self.current_owner].closing {
            Self::finalizer_roots(finalizer, &mut self.scratch);
            return self.raise_defect(DefectSpec::arena_owner_closed());
        }
        self.owners[self.current_owner].finalizers.push(finalizer);
        self.store(binding, Value::Unit)
    }

    fn begin_close_scope(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome<'p>,
    ) -> Result<(), Error> {
        let finalizers = mem::take(&mut self.owners[owner].finalizers);
        if finalizers.is_empty() {
            self.close_owner(owner)?;
            self.current_owner = previous;
            return self.complete_scope_outcome(outcome, None);
        }
        self.owners[owner].closing = true;
        self.current_owner = owner;
        let state = Box::new(ClosingScopeState {
            owner,
            previous,
            remaining: finalizers,
            outcome,
            finalizer_failure: None,
        });
        self.step_close_scope(state)
    }

    fn step_close_scope(&mut self, mut state: Box<ClosingScopeState<'p>>) -> Result<(), Error> {
        let Some(finalizer) = state.remaining.pop() else {
            let owner = state.owner;
            let previous = state.previous;
            let outcome = state.outcome;
            let finalizer_failure = state.finalizer_failure;
            self.close_owner(owner)?;
            self.current_owner = previous;
            return self.complete_scope_outcome(outcome, finalizer_failure);
        };
        self.current_owner = state.owner;
        match finalizer {
            Finalizer::Task(task) => {
                self.scratch.push(task);
                self.scratch.push(Value::Unit);
                self.await_task(Destination::ClosingScope { state })
            }
            Finalizer::WithCause(func) => {
                let arg = match &state.outcome {
                    ScopeOutcome::Failure(cause) => {
                        let dup = self.heap.dup_cause(cause)?;
                        self.heap
                            .construct("Prelude.Option.Some", vec![dup.into_value()])?
                    }
                    ScopeOutcome::Normal { .. } | ScopeOutcome::Discard(_) => {
                        self.heap.construct("Prelude.Option.None", vec![])?
                    }
                };
                self.scratch.extend([func, arg]);
                self.apply(Destination::ClosingScope { state })
            }
            Finalizer::WithExit(func) => {
                let exit = match &state.outcome {
                    ScopeOutcome::Normal { value, .. } => {
                        let dup = self.heap.dup(value)?;
                        self.heap.allocate(Data::Exit(ExitData::Success(dup)))
                    }
                    ScopeOutcome::Failure(cause) => {
                        let dup = self.heap.dup_cause(cause)?;
                        self.heap
                            .allocate(Data::Exit(ExitData::Failure(dup.into_value())))
                    }
                    ScopeOutcome::Discard(_) => self
                        .heap
                        .allocate(Data::Exit(ExitData::Success(Value::Unit))),
                };
                self.scratch.extend([func, exit]);
                self.apply(Destination::ClosingScope { state })
            }
            Finalizer::Release { value, release_fn } => {
                self.scratch.extend([release_fn, value]);
                self.apply(Destination::ClosingScope { state })
            }
        }
    }

    fn complete_scope_outcome(
        &mut self,
        outcome: ScopeOutcome<'p>,
        finalizer_failure: Option<CauseToken>,
    ) -> Result<(), Error> {
        match outcome {
            ScopeOutcome::Normal { binding, value } => {
                if let Some(cause) = finalizer_failure {
                    self.heap.drop_value(value)?;
                    self.failure = Some(cause);
                    self.failure_elected = false;
                } else {
                    self.store(binding, value)?;
                }
            }
            ScopeOutcome::Failure(cause) => {
                if let Some(fin_cause) = finalizer_failure {
                    self.heap.drop_cause(fin_cause)?;
                }
                self.failure = Some(cause);
                self.failure_elected = false;
            }
            ScopeOutcome::Discard(discard) => {
                if let Some(fin_cause) = finalizer_failure {
                    let mut discard = *discard;
                    let preserved = Self::take_saved_failure(&mut discard.next);
                    if let Some(prev) = preserved {
                        self.heap.drop_cause(fin_cause)?;
                        self.failure = Some(prev);
                        self.failure_elected = false;
                    } else {
                        self.failure = Some(fin_cause);
                        self.failure_elected = false;
                    }
                    let mut roots = Vec::new();
                    self.drain_transfer_next_roots(&mut discard.next, &mut roots);
                    self.heap.drop_values(roots)?;
                    discard.next = TransferNext::Raise;
                    self.discard = Some(discard);
                } else {
                    self.discard = Some(*discard);
                }
            }
        }
        Ok(())
    }

    fn take_saved_failure(next: &mut TransferNext<'p>) -> Option<CauseToken> {
        match next {
            TransferNext::ResumeRunning { saved_failure } => saved_failure.take(),
            TransferNext::Chain(chained) => Self::take_saved_failure(&mut chained.next),
            _ => None,
        }
    }
}
