// Race, Timeout, parent-interrupt propagation, and coordinator/group cleanup.

impl Machine<'_, '_> {
    fn step_coord_race_mut(
        &mut self,
        coord: &mut CoordState,
        child_id: usize,
    ) -> Result<bool, Error> {
        let parent = coord.parent_fiber;
        let CoordKind::Race {
            success_only,
            children,
            pending,
            winner,
            failures,
            stopping,
        } = &mut coord.kind
        else {
            unreachable!()
        };
        *pending = pending.saturating_sub(1);
        let child_is_ok = matches!(
            self.fiber_table[&child_id].outcome,
            FiberOutcome::Success(_)
        );
        if !*stopping && (child_is_ok || !*success_only) {
            *stopping = true;
            *winner = Some(match &self.fiber_table[&child_id].outcome {
                FiberOutcome::Success(v) => {
                    let rep = v.representation();
                    FiberOutcome::Success(self.heap.dup(&rep)?)
                }
                FiberOutcome::Failure(c) => FiberOutcome::Failure(self.heap.dup_cause(c)?),
                FiberOutcome::Running => FiberOutcome::Running,
            });
            for &other in children.iter() {
                if other != child_id {
                    self.request_interrupt(other, parent as u64)?;
                }
            }
        } else if !*stopping && let FiberOutcome::Failure(c) = &self.fiber_table[&child_id].outcome
        {
            let dup = self.heap.dup_cause(c)?;
            *failures = Some(match failures.take() {
                None => dup,
                Some(prev) => {
                    let cat = primitives::concat_causes(self.heap, &prev, &dup, &mut self.fuel);
                    self.heap.drop_cause(prev)?;
                    self.heap.drop_cause(dup)?;
                    cat?
                }
            });
        }
        Ok(*pending == 0)
    }

    fn finish_coord_race(
        &mut self,
        parent: usize,
        binding: Local,
        winner: Option<FiberOutcome>,
        failures: Option<CauseToken>,
    ) -> Result<(), Error> {
        if let Some(w) = winner {
            if let Some(f) = failures {
                self.heap.drop_cause(f)?;
            }
            match w {
                FiberOutcome::Success(v) => self.wake_fiber_with_value(parent, binding, v),
                FiberOutcome::Failure(c) => self.wake_fiber_with_failure(parent, c),
                FiberOutcome::Running => self.wake_fiber_with_value(parent, binding, Value::Unit),
            }
        } else {
            let cause = match failures {
                Some(f) => match self.heap.elect_failure(&f, &mut self.fuel) {
                    Ok(Some(e)) => {
                        self.heap.drop_cause(f)?;
                        e
                    }
                    Ok(None) => f,
                    Err(err) => {
                        self.heap.drop_cause(f)?;
                        return Err(err);
                    }
                },
                None => self.heap.cause(&[], &mut self.fuel)?,
            };
            self.wake_fiber_with_failure(parent, cause)
        }
    }

    fn on_timeout_timer(&mut self, coord_id: usize) -> Result<(), Error> {
        let Some(coord) = self.coords.get_mut(&coord_id) else {
            return Ok(());
        };
        let parent = coord.parent_fiber;
        let CoordKind::Timeout {
            child,
            ref mut timed_out,
            ..
        } = coord.kind
        else {
            return Ok(());
        };
        *timed_out = true;
        self.request_interrupt(child, parent as u64)
    }

    #[expect(clippy::too_many_arguments, reason = "timeout completion dispatch")]
    fn finish_coord_timeout(
        &mut self,
        parent: usize,
        binding: Local,
        child: usize,
        timer_id: usize,
        kind: TimeoutKind,
        timed_out: bool,
        interrupted_by: Option<u64>,
    ) -> Result<(), Error> {
        self.cancel_timer(timer_id);
        if let Some(by) = interrupted_by {
            if let TimeoutKind::OrElse(fb) = kind {
                self.heap.drop_value(fb)?;
            }
            let cause = self.interrupt_cause(by)?;
            return self.wake_fiber_with_failure(parent, cause);
        }
        if timed_out {
            match kind {
                TimeoutKind::Option => {
                    let none = self.heap.construct("Prelude.Option.None", vec![])?;
                    self.wake_fiber_with_value(parent, binding, none)
                }
                TimeoutKind::ThrowError => {
                    let err = self
                        .heap
                        .construct("Prelude.TimeoutError.TimeoutError", vec![])?;
                    let cause = self.heap.cause(&[Reason::Fail(&err)], &mut self.fuel);
                    self.heap.drop_value(err)?;
                    self.wake_fiber_with_failure(parent, cause?)
                }
                TimeoutKind::OrElse(fallback) => {
                    self.wake_fiber_run_task(parent, binding, fallback)
                }
            }
        } else {
            let is_option = matches!(kind, TimeoutKind::Option);
            if let TimeoutKind::OrElse(fb) = kind {
                self.heap.drop_value(fb)?;
            }
            match &self.fiber_table[&child].outcome {
                FiberOutcome::Success(v) => {
                    let rep = v.representation();
                    let dup = self.heap.dup(&rep)?;
                    let out = if is_option {
                        self.heap.construct("Prelude.Option.Some", vec![dup])?
                    } else {
                        dup
                    };
                    self.wake_fiber_with_value(parent, binding, out)
                }
                FiberOutcome::Failure(c) => {
                    let dup = self.heap.dup_cause(c)?;
                    self.wake_fiber_with_failure(parent, dup)
                }
                FiberOutcome::Running => Ok(()),
            }
        }
    }

    fn wake_fiber_run_task(
        &mut self,
        target: usize,
        binding: Local,
        task: Value,
    ) -> Result<(), Error> {
        let Some(rec) = self.fiber_table.get_mut(&target) else {
            return self.heap.drop_value(task);
        };
        if rec.parked.take().is_none() {
            return self.heap.drop_value(task);
        }
        let caller = self.active_fiber;
        self.switch_to_fiber(target);
        self.scratch.extend([task, Value::Unit]);
        let res = self.await_task_fresh(Destination::Bind(binding));
        self.switch_to_fiber(caller);
        res?;
        self.run_queue.push_back(target);
        Ok(())
    }

    fn interrupt_coord_parent(
        &mut self,
        coord_id: usize,
        parent: usize,
        by: u64,
    ) -> Result<(), Error> {
        let Some(coord) = self.coords.get_mut(&coord_id) else {
            return Ok(());
        };
        match &mut coord.kind {
            CoordKind::All {
                queued,
                children,
                first_failure,
                stopping,
                ..
            } => {
                *stopping = true;
                let dropped: Vec<Value> = queued.drain(..).map(|(_, t)| t).collect();
                let to_stop = children.clone();
                let needs_cause = first_failure.is_none();
                self.heap.drop_values(dropped)?;
                if needs_cause {
                    let cause = self.interrupt_cause(by)?;
                    if let Some(c) = self.coords.get_mut(&coord_id)
                        && let CoordKind::All { first_failure, .. } = &mut c.kind
                    {
                        *first_failure = Some(cause);
                    } else {
                        self.heap.drop_cause(cause)?;
                    }
                }
                for child in to_stop {
                    self.request_interrupt(child, parent as u64)?;
                }
            }
            CoordKind::Race {
                children,
                winner,
                stopping,
                ..
            } => {
                *stopping = true;
                let to_stop = children.clone();
                if winner.is_none() {
                    let cause = self.interrupt_cause(by)?;
                    if let Some(c) = self.coords.get_mut(&coord_id)
                        && let CoordKind::Race { winner, .. } = &mut c.kind
                    {
                        *winner = Some(FiberOutcome::Failure(cause));
                    } else {
                        self.heap.drop_cause(cause)?;
                    }
                }
                for child in to_stop {
                    self.request_interrupt(child, parent as u64)?;
                }
            }
            CoordKind::Timeout {
                child,
                timer_id,
                interrupted_by,
                ..
            } => {
                let c = *child;
                let tid = *timer_id;
                interrupted_by.get_or_insert(by);
                self.cancel_timer(tid);
                self.request_interrupt(c, parent as u64)?;
            }
            CoordKind::WaitAll { .. } | CoordKind::OwnerHookWait { .. } => {}
        }
        Ok(())
    }

    fn fiber_services_for_spawn(&mut self, fid: usize) -> Result<Vec<Value>, Error> {
        if fid == self.active_fiber {
            return self.snapshot_active_services();
        }
        let caller = self.active_fiber;
        self.switch_to_fiber(fid);
        let res = self.snapshot_active_services();
        self.switch_to_fiber(caller);
        res
    }

    fn fiber_refs_for_spawn(&mut self, fid: usize) -> Result<BTreeMap<String, Value>, Error> {
        if fid == self.active_fiber {
            return self.snapshot_fiber_refs();
        }
        let caller = self.active_fiber;
        self.switch_to_fiber(fid);
        let res = self.snapshot_fiber_refs();
        self.switch_to_fiber(caller);
        res
    }

    fn fiber_evidence(&self, fid: usize) -> Option<OwnerEvidence> {
        if fid == self.active_fiber {
            self.current_evidence
        } else {
            self.parked_fibers
                .get(&fid)
                .and_then(|c| c.current_evidence)
        }
    }

    fn cleanup_coords_and_groups(&mut self, roots: &mut Vec<Value>) {
        for def in mem::take(&mut self.deferreds).into_values() {
            if let Some(outcome) = def.outcome {
                match outcome {
                    DeferredOutcome::Exit(ExitData::Success(v) | ExitData::Failure(v))
                    | DeferredOutcome::Task(v) => roots.push(v),
                }
            }
        }
        for cell in mem::take(&mut self.cached_cells).into_values() {
            roots.push(cell.task);
            if let CachedEntry::Ready { value, .. } = cell.entry {
                roots.push(value);
            }
        }
        for coord in mem::take(&mut self.coords).into_values() {
            match coord.kind {
                CoordKind::All {
                    queued,
                    results,
                    first_failure,
                    ..
                } => {
                    roots.extend(queued.into_iter().map(|(_, t)| t));
                    roots.extend(results.into_iter().flatten());
                    if let Some(c) = first_failure {
                        roots.push(c.into_value());
                    }
                }
                CoordKind::Race {
                    winner, failures, ..
                } => {
                    if let Some(w) = winner {
                        match w {
                            FiberOutcome::Success(v) => roots.push(v),
                            FiberOutcome::Failure(c) => roots.push(c.into_value()),
                            FiberOutcome::Running => {}
                        }
                    }
                    if let Some(f) = failures {
                        roots.push(f.into_value());
                    }
                }
                CoordKind::Timeout { kind, .. } => {
                    if let TimeoutKind::OrElse(fb) = kind {
                        roots.push(fb);
                    }
                }
                CoordKind::WaitAll { done_value, .. } => roots.push(done_value),
                CoordKind::OwnerHookWait { .. } => {}
            }
        }
        for grp in mem::take(&mut self.fiber_groups).into_values() {
            roots.extend(grp.entries.into_values().filter_map(|e| e.key));
            if let Some(f) = grp.first_failure {
                roots.push(f.into_value());
            }
        }
    }
}
