// Lazy cells, resource acquisition, finalizer registration, and scope-close entry points.
impl<'p> Machine<'p, '_> {
    fn control_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<bool, Error> {
        match kind {
            Intrinsic::LazyMake | Intrinsic::LazyMakeLocal => {
                let task = self.scratch.pop().expect("lazy task");
                let local_owner = matches!(kind, Intrinsic::LazyMakeLocal).then_some(self.active_fiber);
                let root = self.heap.allocate(Data::Lazy {
                    execution: self.execution,
                    local_owner,
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
                let services = self.snapshot_active_services()?;
                self.register_scope_finalizer(binding, Finalizer::Task { task, services })?;
            }
            Intrinsic::ScopeAddFinalizerExit => {
                let func = self.scratch.pop().expect("exit finalizer");
                let services = self.snapshot_active_services()?;
                self.register_scope_finalizer(binding, Finalizer::WithCause { func, services })?;
            }
            Intrinsic::ScopeAddClose => {
                let func = self.scratch.pop().expect("close callback");
                let services = self.snapshot_active_services()?;
                self.register_scope_finalizer(binding, Finalizer::WithExit { func, services })?;
            }
            Intrinsic::AcquireRelease => {
                let release = self.scratch.pop().expect("release fn");
                let acquire = self.scratch.pop().expect("acquire task");
                let owner = self.current_owner;
                if self.owners[owner].is_finalizing_or_closed() {
                    self.scratch.extend([acquire, release]);
                    self.raise_defect(DefectSpec::arena_owner_closed())?;
                    return Ok(true);
                }
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.scratch.extend([acquire, Value::Unit]);
                self.await_task_fresh(Destination::RestoreMask {
                    saved_mask,
                    check_on_exit: true,
                    next: Box::new(Destination::AcquireRegister {
                        owner,
                        release,
                        binding,
                    }),
                })?;
            }
            Intrinsic::Bracket => {
                let release_fn = self.scratch.pop().expect("bracket release");
                let use_fn = self.scratch.pop().expect("bracket use");
                let acquire = self.scratch.pop().expect("bracket acquire");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.scratch.extend([acquire, Value::Unit]);
                self.await_task_fresh(Destination::RestoreMask {
                    saved_mask,
                    check_on_exit: true,
                    next: Box::new(Destination::BracketUse {
                        use_fn,
                        release_fn,
                        binding,
                    }),
                })?;
            }
            Intrinsic::Uninterruptible => {
                let task = self.scratch.pop().expect("masked task");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.scratch.extend([task, Value::Unit]);
                self.await_task_fresh(Destination::RestoreMask {
                    saved_mask,
                    check_on_exit: true,
                    next: Box::new(Destination::Bind(binding)),
                })?;
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

    fn destination_can_reset_local(&self, dest: &Destination<'p>) -> bool {
        match dest {
            Destination::Handle { activation, .. } => self
                .activations
                .get(activation)
                .is_some_and(|act| act.operations.iter().any(|op| op.class != OpClass::Tail)),
            Destination::CaptureExit { .. } | Destination::CatchCause { .. } => true,
            Destination::RestoreMask { next, .. }
            | Destination::WithServices { next, .. }
            | Destination::CachedComplete { next, .. } => self.destination_can_reset_local(next),
            _ => false,
        }
    }

    fn has_local_reset_frame(&self) -> bool {
        self.frames
            .iter()
            .any(|frame| self.destination_can_reset_local(&frame.destination))
    }

    fn lazy_force(&mut self, binding: Local) -> Result<(), Error> {
        enum LazyAction {
            Return(Value),
            FailCause(CauseToken),
            Wait,
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
        let active = self.active_fiber;
        let has_reset_frame = self.has_local_reset_frame();
        let action = {
            let (slot, local_owner) = self.heap.lazy_slot_mut(self.execution, &handle)?;
            match slot {
                Some(LazyState::DoneOk(value)) => {
                    let rep = value.representation();
                    LazyAction::Return(self.heap.dup(&rep)?)
                }
                Some(LazyState::DoneCause(cause)) => {
                    let id = cause.object_id();
                    LazyAction::FailCause(self.heap.dup_cause_id(id)?)
                }
                Some(LazyState::Running { owner, .. }) => {
                    if *owner == active {
                        return Err(Error::LazyCycle);
                    }
                    LazyAction::Wait
                }
                Some(LazyState::RunningLocal { .. }) => {
                    return Err(Error::LazyCycle);
                }
                Some(LazyState::Pending(_)) => {
                    let Some(LazyState::Pending(task)) = slot.take() else {
                        unreachable!()
                    };
                    if let Some(creator) = local_owner {
                        if active != creator || has_reset_frame {
                            let rep = task.representation();
                            *slot = Some(LazyState::RunningLocal { task: Some(task) });
                            LazyAction::Start(self.heap.dup(&rep)?)
                        } else {
                            // O3 Perceus: move the task out on Pending -> Running
                            // so the task and its captures are owned uniquely and
                            // reclaimed immediately instead of kept alive by the cell.
                            *slot = Some(LazyState::RunningLocal { task: None });
                            LazyAction::Start(task)
                        }
                    } else {
                        let rep = task.representation();
                        *slot = Some(LazyState::Running {
                            task,
                            owner: active,
                        });
                        LazyAction::Start(self.heap.dup(&rep)?)
                    }
                }
                None => return Err(Error::ClosedLazy),
            }
        };
        match action {
            LazyAction::Return(value) => self.store(binding, value),
            LazyAction::FailCause(cause) => {
                if let Some(prev) = self.failure.replace(cause) {
                    self.heap.drop_cause(prev)?;
                }
                self.failure_elected = true;
                Ok(())
            }
            LazyAction::Wait => {
                self.lazy_waiters
                    .entry(handle.id)
                    .or_default()
                    .push((active, binding));
                self.park_current(ParkReason::Lazy { lazy_id: handle.id })
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
        let owner = self.current_owner;
        if self.owners[owner].is_closed() {
            Self::finalizer_roots(finalizer, &mut self.scratch);
            return self.raise_defect(DefectSpec::arena_owner_closed());
        }
        if self.owners[owner].is_finalizing() {
            let exit_cause = match &self.owners[owner].exit_cause {
                Some(c) => Some(self.heap.dup_cause(c)?),
                None => None,
            };
            let saved_mask = self.mask_depth;
            self.mask_depth += 1;
            let next = Destination::RestoreMask {
                saved_mask,
                check_on_exit: true,
                next: Box::new(Destination::Bind(binding)),
            };
            let res = self.run_single_finalizer(finalizer, exit_cause.as_ref(), next);
            if let Some(c) = exit_cause {
                self.heap.drop_cause(c)?;
            }
            return res;
        }
        self.owners[owner].finalizers.push(finalizer);
        self.store(binding, Value::Unit)
    }

    pub(super) fn owner_needs_async_close(&self, owner: usize) -> bool {
        let o = &self.owners[owner];
        !o.is_closed()
            && (!o.hooks.is_empty()
                || !o.children.is_empty()
                || o.watchers > 0
                || !o.finalizers.is_empty()
                || self.foreign.mirrored_owners.contains(&owner))
    }

    pub(super) fn begin_close_scope(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome<'p>,
    ) -> Result<(), Error> {
        self.begin_close_scope_elect(owner, previous, outcome, false)
    }

    pub(super) fn begin_close_scope_elect(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome<'p>,
        elect_on_fail: bool,
    ) -> Result<(), Error> {
        self.begin_close_scope_with_override(owner, previous, outcome, None, elect_on_fail)
    }

    pub(super) fn begin_close_scope_with_override(
        &mut self,
        owner: usize,
        previous: usize,
        outcome: ScopeOutcome<'p>,
        exit_override: Option<CauseToken>,
        elect_on_fail: bool,
    ) -> Result<(), Error> {
        if !self.owner_needs_async_close(owner) {
            if !self.owners[owner].is_closed() {
                self.owners[owner].phase = OwnerPhase::Closing;
            }
            if let Some(c) = exit_override {
                self.heap.drop_cause(c)?;
            }
            self.close_mirrored_owner(owner)?;
            self.close_owner(owner)?;
            self.current_owner = previous;
            return self.complete_scope_outcome(outcome, None, elect_on_fail);
        }
        self.owners[owner].phase = OwnerPhase::Closing;
        self.current_owner = owner;
        let state = Box::new(ClosingScopeState {
            owner,
            previous,
            phase: ClosePhase::Hooks,
            remaining: Vec::new(),
            outcome,
            exit_override,
            finalizer_failure: None,
            elect_on_fail,
            saved_mask: self.mask_depth,
            masked: false,
        });
        self.step_close_scope(state)
    }
}
