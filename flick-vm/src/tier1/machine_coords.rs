//! Structured concurrency coordinators (`parallel*`, `Effect.all`, `Layer.merge_all`,
//! `Fiber.*_all`), `Deferred`, and `Effect.cached` state for tier 1.

use std::collections::VecDeque;

use flick_core::ir::memory::AsyncIntrinsic;

use crate::stdlib;
use crate::tier0::{CauseToken, Data, DefectSpec, Error, ExitData, Value};

use super::machine::{Machine, RegVal};
use super::machine_fibers::{FiberOutcome, FiberWaitKind, FiberWaiter, ParkReason};
use super::machine_layers::LayerBuildTarget;

pub(super) enum DeferredOutcome {
    Exit(ExitData),
    Task(Value),
}

pub(super) struct DeferredState {
    pub(super) outcome: Option<DeferredOutcome>,
    pub(super) waiters: Vec<(usize, usize)>,
}

pub(super) enum CachedEntry {
    Empty,
    Ready {
        value: Value,
        expires_ns: Option<u64>,
    },
}

pub(super) struct CachedCellState {
    pub(super) task: Value,
    pub(super) ttl_ns: Option<u64>,
    pub(super) entry: CachedEntry,
}

pub(super) enum AllMode {
    Pair,
    List,
    Results,
    Exits,
    MergeLayers {
        scopes: Vec<usize>,
        parent_owner: usize,
        or_die: bool,
    },
    JoinAll,
    AwaitAll,
}

pub(super) enum TimeoutKind {
    ThrowError,
    Option,
    OrElse(Value),
}

pub(super) enum CoordKind {
    All {
        mode: AllMode,
        queued: VecDeque<(usize, Value)>,
        children: Vec<usize>,
        results: Vec<Option<Value>>,
        first_failure: Option<CauseToken>,
        stopping: bool,
        pending: usize,
        owner: usize,
    },
    Race {
        success_only: bool,
        children: Vec<usize>,
        pending: usize,
        winner: Option<FiberOutcome>,
        failures: Option<CauseToken>,
        stopping: bool,
    },
    Timeout {
        child: usize,
        timer_id: usize,
        kind: TimeoutKind,
        timed_out: bool,
        interrupted_by: Option<u64>,
    },
    WaitAll {
        pending: usize,
        done_value: Value,
    },
    OwnerHookWait {
        owner: usize,
        pending: usize,
    },
}

pub(super) struct CoordState {
    pub(super) parent_fiber: usize,
    pub(super) target: LayerBuildTarget,
    pub(super) kind: CoordKind,
}

impl Machine<'_, '_> {
    pub(super) fn alloc_coord_id(&mut self) -> usize {
        let id = self.next_coord;
        self.next_coord = self.next_coord.checked_add(1).expect("coord id overflow");
        id
    }

    pub(super) fn parallel_race_intrinsic(
        &mut self,
        op: AsyncIntrinsic,
        dst: usize,
    ) -> Result<(), Error> {
        match op {
            AsyncIntrinsic::Parallel => {
                let b = self.scratch.pop().expect("parallel b");
                let a = self.scratch.pop().expect("parallel a");
                self.start_parallel_tasks(vec![a, b], AllMode::Pair, 0, dst)
            }
            AsyncIntrinsic::ParallelAll => self.exec_parallel_all(dst),
            AsyncIntrinsic::ParallelResults => self.exec_parallel_list(AllMode::Results, 0, dst),
            AsyncIntrinsic::ParallelExits => self.exec_parallel_list(AllMode::Exits, 0, dst),
            AsyncIntrinsic::EffectAll => self.exec_effect_all(dst),
            AsyncIntrinsic::RaceSuccess => self.exec_race_pair(true, dst),
            AsyncIntrinsic::RaceFirst => self.exec_race_pair(false, dst),
            AsyncIntrinsic::RaceAll => self.exec_race_list(true, dst),
            AsyncIntrinsic::RaceAllFirst => self.exec_race_list(false, dst),
            AsyncIntrinsic::Timeout => self.exec_timeout(TimeoutKind::ThrowError, false, dst),
            AsyncIntrinsic::TimeoutOption => self.exec_timeout(TimeoutKind::Option, false, dst),
            AsyncIntrinsic::TimeoutOrElse => {
                let task = self.scratch.pop().expect("task");
                let fb = self.scratch.pop().expect("fallback");
                let dur = self.scratch.pop().expect("dur");
                self.scratch.extend([dur, task]);
                self.exec_timeout(TimeoutKind::OrElse(fb), true, dst)
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

    pub(super) fn exec_fiber_all(&mut self, mode: AllMode, dst: usize) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("fibers");
        let ids = self.extract_fiber_list(list_val)?;
        if ids.is_empty() {
            let empty = self.heap.construct("$list", vec![])?;
            return self.store_reg(dst, RegVal::Value(empty));
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
            return self.store_reg(dst, RegVal::Value(list));
        }
        let stopping = first_failure.is_some();
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target: LayerBuildTarget::Local(dst),
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

    pub(super) fn exec_fiber_interrupt_all(
        &mut self,
        by: Option<u64>,
        dst: usize,
    ) -> Result<(), Error> {
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
            return self.store_reg(dst, RegVal::Value(Value::Unit));
        }
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target: LayerBuildTarget::Local(dst),
                kind: CoordKind::WaitAll {
                    pending,
                    done_value: Value::Unit,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    fn exec_parallel_all(&mut self, dst: usize) -> Result<(), Error> {
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
        let tasks = stdlib::runtime::coop::clone_list_values(self.heap, &list_val)?;
        self.heap.drop_value(list_val)?;
        #[expect(clippy::cast_sign_loss, reason = "checked limit > 0")]
        self.start_parallel_tasks(tasks, AllMode::List, limit as usize, dst)
    }

    fn exec_parallel_list(&mut self, mode: AllMode, limit: usize, dst: usize) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("parallel tasks");
        let tasks = stdlib::runtime::coop::clone_list_values(self.heap, &list_val)?;
        self.heap.drop_value(list_val)?;
        self.start_parallel_tasks(tasks, mode, limit, dst)
    }

    fn exec_effect_all(&mut self, dst: usize) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("effect all tasks");
        let conc_val = self.scratch.pop().expect("concurrency");
        let limit = self.decode_concurrency(&conc_val)?;
        self.heap.drop_value(conc_val)?;
        let tasks = stdlib::runtime::coop::clone_list_values(self.heap, &list_val)?;
        self.heap.drop_value(list_val)?;
        self.start_parallel_tasks(tasks, AllMode::List, limit, dst)
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
        dst: usize,
    ) -> Result<(), Error> {
        if tasks.is_empty() {
            let empty = self.heap.construct("$list", vec![])?;
            return self.store_reg(dst, RegVal::Value(empty));
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
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target: LayerBuildTarget::Local(dst),
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
