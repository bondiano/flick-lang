// Parallel, race, timeout, Effect.all, Fiber.*_all, list cloning, and fiber-id decoders.

impl Machine<'_, '_> {
    fn parallel_race_intrinsic(&mut self, op: AsyncIntrinsic, binding: Local) -> Result<(), Error> {
        match op {
            AsyncIntrinsic::Parallel => self.exec_parallel_pair(binding),
            AsyncIntrinsic::ParallelAll => self.exec_parallel_all(binding),
            AsyncIntrinsic::ParallelResults => {
                self.exec_parallel_list(AllMode::Results, 0, binding)
            }
            AsyncIntrinsic::ParallelExits => self.exec_parallel_list(AllMode::Exits, 0, binding),
            AsyncIntrinsic::EffectAll => self.exec_effect_all(binding),
            AsyncIntrinsic::RaceSuccess => self.exec_race_pair(true, binding),
            AsyncIntrinsic::RaceFirst => self.exec_race_pair(false, binding),
            AsyncIntrinsic::RaceAll => self.exec_race_list(true, binding),
            AsyncIntrinsic::RaceAllFirst => self.exec_race_list(false, binding),
            AsyncIntrinsic::Timeout => self.exec_timeout(TimeoutKind::ThrowError, false, binding),
            AsyncIntrinsic::TimeoutOption => self.exec_timeout(TimeoutKind::Option, false, binding),
            AsyncIntrinsic::TimeoutOrElse => {
                let task = self.scratch.pop().expect("task");
                let fb = self.scratch.pop().expect("fallback");
                let dur = self.scratch.pop().expect("dur");
                self.scratch.extend([dur, task]);
                self.exec_timeout(TimeoutKind::OrElse(fb), true, binding)
            }
            _ => unreachable!("non parallel/race/timeout intrinsic"),
        }
    }

    fn extract_fiber_list(&mut self, list_val: Value) -> Result<Vec<usize>, Error> {
        let (backing_id, offset, len) = self.heap.list_view(&list_val)?;
        let mut ids = Vec::with_capacity(len);
        for i in offset..offset + len {
            let Data::Tagged { fields, .. } = &self.heap.object(backing_id)?.data else {
                unreachable!("list_view verified tagged list")
            };
            ids.push(self.fiber_id_of(&fields[i])?);
        }
        self.heap.drop_value(list_val)?;
        Ok(ids)
    }

    fn exec_fiber_all(&mut self, mode: AllMode, binding: Local) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("fibers");
        let ids = self.extract_fiber_list(list_val)?;
        if ids.is_empty() {
            let empty = self.heap.construct("$list", vec![])?;
            return self.store(binding, empty);
        }
        let coord_id = self.alloc_coord_id();
        let parent = self.active_fiber;
        let mut pending = 0;
        let mut results: Vec<Option<Value>> = (0..ids.len()).map(|_| None).collect();
        let mut first_failure = None;
        for (idx, fid) in ids.iter().copied().enumerate() {
            let running = self
                .fiber_table
                .get(&fid)
                .is_some_and(|r| matches!(r.outcome, FiberOutcome::Running));
            if running {
                if let Some(rec) = self.fiber_table.get_mut(&fid) {
                    rec.waiters.push(FiberWaiter {
                        fiber: parent,
                        kind: FiberWaitKind::CoordDone {
                            coord_id,
                            index: idx,
                        },
                    });
                }
                pending += 1;
            } else if matches!(mode, AllMode::AwaitAll) {
                results[idx] = Some(self.fiber_outcome_to_exit(fid)?);
            } else {
                match &self.fiber_table[&fid].outcome {
                    FiberOutcome::Success(v) => {
                        let rep = v.representation();
                        results[idx] = Some(self.heap.dup(&rep)?);
                    }
                    FiberOutcome::Failure(c) if first_failure.is_none() => {
                        first_failure = Some(self.heap.dup_cause(c)?);
                    }
                    _ => {}
                }
            }
        }
        if pending == 0 {
            if let Some(cause) = first_failure {
                self.heap
                    .drop_values(results.into_iter().flatten().collect())?;
                self.failure = Some(cause);
                self.failure_elected = true;
                return Ok(());
            }
            let items: Vec<Value> = results
                .into_iter()
                .map(|o| o.unwrap_or(Value::Unit))
                .collect();
            let list = self.heap.construct("$list", items)?;
            return self.store(binding, list);
        }
        let stopping = first_failure.is_some();
        let target = LayerBuildTarget::Local(binding);
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target,
                kind: CoordKind::All {
                    mode,
                    queued: VecDeque::new(),
                    children: ids,
                    results,
                    first_failure,
                    stopping,
                    pending,
                    owner: self.current_owner,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    fn exec_fiber_interrupt_all(&mut self, by: Option<u64>, binding: Local) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("fibers");
        let ids = self.extract_fiber_list(list_val)?;
        let interruptor = by.unwrap_or(self.active_fiber as u64);
        for &fid in &ids {
            self.request_interrupt(fid, interruptor)?;
        }
        let coord_id = self.alloc_coord_id();
        let parent = self.active_fiber;
        let mut pending = 0;
        for (idx, fid) in ids.into_iter().enumerate() {
            if self
                .fiber_table
                .get(&fid)
                .is_some_and(|r| matches!(r.outcome, FiberOutcome::Running))
            {
                if let Some(rec) = self.fiber_table.get_mut(&fid) {
                    rec.waiters.push(FiberWaiter {
                        fiber: parent,
                        kind: FiberWaitKind::CoordDone {
                            coord_id,
                            index: idx,
                        },
                    });
                }
                pending += 1;
            }
        }
        if pending == 0 {
            return self.store(binding, Value::Unit);
        }
        let target = LayerBuildTarget::Local(binding);
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target,
                kind: CoordKind::WaitAll {
                    pending,
                    done_value: Value::Unit,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    fn exec_parallel_pair(&mut self, binding: Local) -> Result<(), Error> {
        let b = self.scratch.pop().expect("parallel b");
        let a = self.scratch.pop().expect("parallel a");
        self.start_parallel_tasks(vec![a, b], AllMode::Pair, 0, binding)
    }

    fn exec_parallel_all(&mut self, binding: Local) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("parallel_all tasks");
        let limit_val = self.scratch.pop().expect("parallel_all limit");
        let Value::Int(limit) = limit_val else {
            self.heap.drop_value(list_val)?;
            return Err(Error::ExpectedInt);
        };
        if limit <= 0 {
            self.heap.drop_value(list_val)?;
            return self.raise_defect(DefectSpec::custom(format!(
                "RangeError: parallel_all needs a positive finite integer limit, not {limit}"
            )));
        }
        let tasks = self.clone_list_values(&list_val)?;
        self.heap.drop_value(list_val)?;
        #[expect(clippy::cast_sign_loss, reason = "checked limit > 0")]
        self.start_parallel_tasks(tasks, AllMode::List, limit as usize, binding)
    }

    fn exec_parallel_list(
        &mut self,
        mode: AllMode,
        limit: usize,
        binding: Local,
    ) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("parallel tasks");
        let tasks = self.clone_list_values(&list_val)?;
        self.heap.drop_value(list_val)?;
        self.start_parallel_tasks(tasks, mode, limit, binding)
    }

    fn exec_effect_all(&mut self, binding: Local) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("effect all tasks");
        let conc_val = self.scratch.pop().expect("concurrency");
        let limit = self.decode_concurrency(&conc_val)?;
        self.heap.drop_value(conc_val)?;
        let tasks = self.clone_list_values(&list_val)?;
        self.heap.drop_value(list_val)?;
        self.start_parallel_tasks(tasks, AllMode::List, limit, binding)
    }

    fn decode_concurrency(&self, val: &Value) -> Result<usize, Error> {
        let Value::Object(tok) = val else {
            return Ok(1);
        };
        let Data::Tagged { tag, fields, .. } = &self.heap.object(tok.id)?.data else {
            return Ok(1);
        };
        if tag.ends_with(".Sequential") {
            Ok(1)
        } else if tag.ends_with(".Unbounded") {
            Ok(0)
        } else if tag.ends_with(".Bounded") {
            match fields.first() {
                Some(Value::Int(n)) if *n > 0 =>
                {
                    #[expect(clippy::cast_sign_loss, reason = "checked > 0")]
                    Ok(*n as usize)
                }
                _ => Ok(1),
            }
        } else {
            Ok(1)
        }
    }

    fn start_parallel_tasks(
        &mut self,
        tasks: Vec<Value>,
        mode: AllMode,
        limit: usize,
        binding: Local,
    ) -> Result<(), Error> {
        if tasks.is_empty() {
            let empty = self.heap.construct("$list", vec![])?;
            return self.store(binding, empty);
        }
        match self.check_active_interrupt() {
            Ok(true) => {
                self.heap.drop_values(tasks)?;
                return Ok(());
            }
            Ok(false) => {}
            Err(err) => {
                self.heap.drop_values(tasks)?;
                return Err(err);
            }
        }
        let total = tasks.len();
        let initial = if limit == 0 { total } else { limit.min(total) };
        let coord_id = self.alloc_coord_id();
        let parent = self.active_fiber;
        let owner = self.current_owner;
        let ev = self.current_evidence;
        let mut iter = tasks.into_iter().enumerate();
        let mut children = Vec::with_capacity(initial);
        for _ in 0..initial {
            let (idx, t) = iter.next().expect("initial task");
            let services = self.snapshot_active_services()?;
            let refs = self.snapshot_fiber_refs()?;
            let cid = self.spawn_raw_fiber(owner, ev, t, services, refs, 0)?;
            if let Some(rec) = self.fiber_table.get_mut(&cid) {
                rec.coord_parent = Some((coord_id, idx));
            }
            children.push(cid);
        }
        let queued: VecDeque<(usize, Value)> = iter.collect();
        let target = LayerBuildTarget::Local(binding);
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target,
                kind: CoordKind::All {
                    mode,
                    queued,
                    children,
                    results: (0..total).map(|_| None).collect(),
                    first_failure: None,
                    stopping: false,
                    pending: initial,
                    owner,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }
}
