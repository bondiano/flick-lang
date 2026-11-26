// Deferred, TestClock, and Random.with_seed intrinsics (Stage 4A).

impl Machine<'_, '_> {
    fn deferred_intrinsic(&mut self, op: AsyncIntrinsic, binding: Local) -> Result<(), Error> {
        match op {
            AsyncIntrinsic::DeferredMake => {
                self.release_operands()?;
                let id = self.next_deferred;
                self.next_deferred = self
                    .next_deferred
                    .checked_add(1)
                    .expect("deferred id overflow");
                self.deferreds.insert(
                    id,
                    DeferredState {
                        outcome: None,
                        waiters: Vec::new(),
                    },
                );
                let val = self.heap.allocate(Data::Deferred {
                    execution: self.execution,
                    id,
                });
                self.store(binding, val)
            }
            AsyncIntrinsic::DeferredIsDone => {
                let d_val = self.scratch.pop().expect("deferred");
                let id = self.deferred_id_of(&d_val)?;
                self.heap.drop_value(d_val)?;
                let done = self.deferreds.get(&id).is_some_and(|d| d.outcome.is_some());
                self.store(binding, Value::Bool(done))
            }
            AsyncIntrinsic::DeferredPoll => self.exec_deferred_poll(binding),
            AsyncIntrinsic::DeferredAwait => self.exec_deferred_await(binding),
            AsyncIntrinsic::DeferredDone => {
                let d_val = self.scratch.pop().expect("deferred");
                let ex_val = self.scratch.pop().expect("exit");
                let id = self.deferred_id_of(&d_val)?;
                self.heap.drop_value(d_val)?;
                let exit_data = self.extract_exit_data(ex_val)?;
                self.complete_deferred(id, DeferredOutcome::Exit(exit_data), binding)
            }
            AsyncIntrinsic::DeferredCompleteWith => {
                let d_val = self.scratch.pop().expect("deferred");
                let task = self.scratch.pop().expect("task");
                let id = self.deferred_id_of(&d_val)?;
                self.heap.drop_value(d_val)?;
                self.complete_deferred(id, DeferredOutcome::Task(task), binding)
            }
            AsyncIntrinsic::DeferredInterruptWith => {
                let d_val = self.scratch.pop().expect("deferred");
                let fid_val = self.scratch.pop().expect("fiber id");
                let id = self.deferred_id_of(&d_val)?;
                let fid = Self::decode_fiber_float(&fid_val)?;
                self.heap.drop_values(vec![d_val, fid_val])?;
                let cause = self.interrupt_cause(fid)?;
                self.complete_deferred(
                    id,
                    DeferredOutcome::Exit(ExitData::Failure(cause.into_value())),
                    binding,
                )
            }
            _ => unreachable!("handled above"),
        }
    }

    fn exec_deferred_poll(&mut self, binding: Local) -> Result<(), Error> {
        let d_val = self.scratch.pop().expect("deferred");
        let id = self.deferred_id_of(&d_val)?;
        self.heap.drop_value(d_val)?;
        let out = match self.deferreds.get(&id).and_then(|d| d.outcome.as_ref()) {
            None => self.heap.construct("Prelude.Option.None", vec![])?,
            Some(DeferredOutcome::Task(t)) => {
                let rep = t.representation();
                let dup = self.heap.dup(&rep)?;
                self.heap.construct("Prelude.Option.Some", vec![dup])?
            }
            Some(DeferredOutcome::Exit(ex)) => {
                let ex_dup = match ex {
                    ExitData::Success(v) => ExitData::Success(self.heap.dup(&v.representation())?),
                    ExitData::Failure(v) => ExitData::Failure(self.heap.dup(&v.representation())?),
                };
                let task = self.heap.allocate(Data::ExitTask(ex_dup));
                self.heap.construct("Prelude.Option.Some", vec![task])?
            }
        };
        self.store(binding, out)
    }

    fn exec_deferred_await(&mut self, binding: Local) -> Result<(), Error> {
        let d_val = self.scratch.pop().expect("deferred");
        let id = self.deferred_id_of(&d_val)?;
        self.heap.drop_value(d_val)?;
        if self.check_active_interrupt()? {
            return Ok(());
        }
        match self.deferreds.get(&id).and_then(|d| d.outcome.as_ref()) {
            Some(DeferredOutcome::Exit(ExitData::Success(v))) => {
                let rep = v.representation();
                let dup = self.heap.dup(&rep)?;
                self.store(binding, dup)
            }
            Some(DeferredOutcome::Exit(ExitData::Failure(v))) => {
                let rep = v.representation();
                let dup = self.heap.dup(&rep)?;
                self.failure = Some(CauseToken::from_cause_value(self.heap, dup)?);
                self.failure_elected = true;
                Ok(())
            }
            Some(DeferredOutcome::Task(t)) => {
                let rep = t.representation();
                let dup = self.heap.dup(&rep)?;
                self.scratch.extend([dup, Value::Unit]);
                self.await_task_fresh(Destination::Bind(binding))
            }
            None => {
                let me = self.active_fiber;
                self.deferreds
                    .get_mut(&id)
                    .expect("deferred")
                    .waiters
                    .push((me, binding));
                self.park_current(ParkReason::Deferred { id })
            }
        }
    }

    fn complete_deferred(
        &mut self,
        id: usize,
        outcome: DeferredOutcome,
        binding: Local,
    ) -> Result<(), Error> {
        let already = self.deferreds.get(&id).is_some_and(|d| d.outcome.is_some());
        if already {
            match outcome {
                DeferredOutcome::Exit(ExitData::Success(v) | ExitData::Failure(v))
                | DeferredOutcome::Task(v) => self.heap.drop_value(v)?,
            }
            return self.store(binding, Value::Bool(false));
        }
        let waiters = {
            let state = self.deferreds.get_mut(&id).expect("deferred");
            state.outcome = Some(outcome);
            mem::take(&mut state.waiters)
        };
        for (waiter_fiber, waiter_binding) in waiters {
            let waiting = self.fiber_table.get(&waiter_fiber).is_some_and(
                |r| matches!(r.parked, Some(ParkReason::Deferred { id: did }) if did == id),
            );
            if !waiting {
                continue;
            }
            match self.deferreds[&id].outcome.as_ref().expect("set") {
                DeferredOutcome::Exit(ExitData::Success(v)) => {
                    let rep = v.representation();
                    let dup = self.heap.dup(&rep)?;
                    self.wake_fiber_with_value(waiter_fiber, waiter_binding, dup)?;
                }
                DeferredOutcome::Exit(ExitData::Failure(v)) => {
                    let rep = v.representation();
                    let dup = self.heap.dup(&rep)?;
                    let cause = CauseToken::from_cause_value(self.heap, dup)?;
                    self.wake_fiber_with_failure(waiter_fiber, cause)?;
                }
                DeferredOutcome::Task(t) => {
                    let rep = t.representation();
                    let dup = self.heap.dup(&rep)?;
                    self.wake_fiber_run_task(waiter_fiber, waiter_binding, dup)?;
                }
            }
        }
        self.store(binding, Value::Bool(true))
    }

    fn deferred_id_of(&self, val: &Value) -> Result<usize, Error> {
        let Value::Object(tok) = val else {
            return Err(Error::ExpectedDeferred);
        };
        let Data::Deferred { execution, id } = &self.heap.object(tok.id)?.data else {
            return Err(Error::ExpectedDeferred);
        };
        if *execution != self.execution {
            return Err(Error::ExpectedDeferred);
        }
        Ok(*id)
    }

    fn extract_exit_data(&mut self, val: Value) -> Result<ExitData, Error> {
        stdlib::runtime::coop::extract_exit_data(self.heap, val)
    }

    fn exec_test_clock_adjust(&mut self, binding: Local) -> Result<(), Error> {
        let dur = self.scratch.pop().expect("adjust duration");
        let ns = primitives::duration_sleep_nanos(self.heap, &dur)?.unwrap_or(0);
        self.heap.drop_value(dur)?;
        let target_ns = self.virtual_now_ns.saturating_add(ns);
        self.start_test_clock_advance(target_ns, binding)
    }

    fn exec_test_clock_set_time(&mut self, binding: Local) -> Result<(), Error> {
        let ms_val = self.scratch.pop().expect("set_time millis");
        let ms = match ms_val {
            Value::Float(f) => f,
            Value::Int(i) => f64::from(i),
            _ => {
                self.heap.drop_value(ms_val)?;
                return Err(Error::ExpectedFloat);
            }
        };
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "virtual clock timestamp in non-negative ns"
        )]
        let target_ns = if ms <= 0.0 || !ms.is_finite() {
            0
        } else {
            (ms * 1_000_000.0).round().min((u64::MAX / 2) as f64) as u64
        };
        self.start_test_clock_advance(target_ns, binding)
    }

    fn start_test_clock_advance(&mut self, target_ns: u64, binding: Local) -> Result<(), Error> {
        if !self.test_clock_mode {
            if target_ns > self.virtual_now_ns {
                self.virtual_now_ns = target_ns;
            }
            return self.store(binding, Value::Unit);
        }
        self.test_clock_waiter = Some(TestClockAdvance {
            caller_fiber: self.active_fiber,
            binding,
            target_ns,
        });
        self.park_current(ParkReason::TestClock)
    }

    fn exec_random_with_seed(&mut self, binding: Local) -> Result<(), Error> {
        let task = self.scratch.pop().expect("random task");
        let seed_val = self.scratch.pop().expect("random seed");
        let seed = match seed_val {
            Value::Float(f) => f,
            Value::Int(i) => f64::from(i),
            _ => {
                self.heap.drop_values(vec![seed_val, task])?;
                return Err(Error::ExpectedFloat);
            }
        };
        let id = self.next_rng;
        self.next_rng = self.next_rng.checked_add(1).expect("rng id overflow");
        self.isaac_rngs
            .insert(id, primitives::IsaacRng::from_seed(seed));
        let prev = self.seeded_random.replace(id);
        self.scratch.extend([task, Value::Unit]);
        self.await_task_fresh(Destination::RestoreRandom {
            previous: prev,
            rng_id: id,
            binding,
        })
    }
}
