// Race, timeout, list cloning, and fiber-id decoders.

impl Machine<'_, '_> {
    fn exec_race_pair(&mut self, success_only: bool, binding: Local) -> Result<(), Error> {
        let b = self.scratch.pop().expect("race b");
        let a = self.scratch.pop().expect("race a");
        self.start_race_tasks(vec![a, b], success_only, binding)
    }

    fn exec_race_list(&mut self, success_only: bool, binding: Local) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("race tasks");
        let tasks = self.clone_list_values(&list_val)?;
        self.heap.drop_value(list_val)?;
        self.start_race_tasks(tasks, success_only, binding)
    }

    fn start_race_tasks(
        &mut self,
        tasks: Vec<Value>,
        success_only: bool,
        binding: Local,
    ) -> Result<(), Error> {
        if tasks.is_empty() {
            let cause = self.heap.cause(&[], &mut self.fuel)?;
            self.failure = Some(cause);
            self.failure_elected = true;
            return Ok(());
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
        let coord_id = self.alloc_coord_id();
        let parent = self.active_fiber;
        let owner = self.current_owner;
        let ev = self.current_evidence;
        let pending = tasks.len();
        let mut children = Vec::with_capacity(pending);
        for (idx, t) in tasks.into_iter().enumerate() {
            let services = self.snapshot_active_services()?;
            let refs = self.snapshot_fiber_refs()?;
            let cid = self.spawn_raw_fiber(owner, ev, t, services, refs, 0)?;
            if let Some(rec) = self.fiber_table.get_mut(&cid) {
                rec.coord_parent = Some((coord_id, idx));
            }
            children.push(cid);
        }
        let target = LayerBuildTarget::Local(binding);
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target,
                kind: CoordKind::Race {
                    success_only,
                    children,
                    pending,
                    winner: None,
                    failures: None,
                    stopping: false,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    fn exec_timeout(
        &mut self,
        kind: TimeoutKind,
        from_duration: bool,
        binding: Local,
    ) -> Result<(), Error> {
        let task = self.scratch.pop().expect("timeout task");
        let dur_or_ms = self.scratch.pop().expect("timeout duration");
        let delay_ns = if from_duration {
            primitives::duration_sleep_nanos(self.heap, &dur_or_ms)?
        } else {
            let ms = match dur_or_ms {
                Value::Float(f) => f,
                Value::Int(i) => f64::from(i),
                _ => {
                    self.heap.drop_values(vec![dur_or_ms, task])?;
                    if let TimeoutKind::OrElse(fb) = kind {
                        self.heap.drop_value(fb)?;
                    }
                    return Err(Error::ExpectedFloat);
                }
            };
            if !ms.is_finite() && ms > 0.0 {
                None
            } else if ms <= 0.0 {
                Some(0)
            } else {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    reason = "clamped positive millis to u64 ns"
                )]
                let ns = (ms * 1_000_000.0).round().min((u64::MAX / 2) as f64) as u64;
                Some(ns.max(1))
            }
        };
        self.heap.drop_value(dur_or_ms)?;
        match self.check_active_interrupt() {
            Ok(true) => {
                self.heap.drop_value(task)?;
                if let TimeoutKind::OrElse(fb) = kind {
                    self.heap.drop_value(fb)?;
                }
                return Ok(());
            }
            Ok(false) => {}
            Err(err) => {
                self.heap.drop_value(task)?;
                if let TimeoutKind::OrElse(fb) = kind {
                    self.heap.drop_value(fb)?;
                }
                return Err(err);
            }
        }
        let coord_id = self.alloc_coord_id();
        let parent = self.active_fiber;
        let owner = self.current_owner;
        let ev = self.current_evidence;
        let services = self.snapshot_active_services()?;
        let refs = self.snapshot_fiber_refs()?;
        let child = self.spawn_raw_fiber(owner, ev, task, services, refs, 0)?;
        if let Some(rec) = self.fiber_table.get_mut(&child) {
            rec.coord_parent = Some((coord_id, 0));
        }
        let timer_id = match delay_ns {
            Some(ns) => self.register_timer(ns, TimerAction::FireTimeout { coord_id }),
            None => usize::MAX,
        };
        let target = LayerBuildTarget::Local(binding);
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target,
                kind: CoordKind::Timeout {
                    child,
                    timer_id,
                    kind,
                    timed_out: false,
                    interrupted_by: None,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    fn clone_list_values(&mut self, list_val: &Value) -> Result<Vec<Value>, Error> {
        stdlib::runtime::coop::clone_list_values(self.heap, list_val)
    }

    fn decode_opt_fiber_id(&self, val: &Value) -> Result<Option<u64>, Error> {
        stdlib::runtime::coop::decode_opt_fiber_id(self.heap, val)
    }

    fn decode_fiber_float(val: &Value) -> Result<u64, Error> {
        stdlib::runtime::coop::decode_fiber_float(val)
    }
}
