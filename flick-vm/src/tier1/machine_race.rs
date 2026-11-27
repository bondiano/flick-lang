//! `race_first`, `race_success`, `race_all*`, `timeout*`, parent-interrupt
//! propagation, and coordinator/group cleanup for tier 1.

use std::collections::BTreeMap;
use std::mem;

use crate::stdlib::{self, FuelCounter, primitives};
use crate::tier0::{CauseToken, Error, ExitData, Reason, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_coords::{CachedEntry, CoordKind, CoordState, DeferredOutcome, TimeoutKind};
use super::machine_fibers::{FiberOutcome, ParkReason, TimerAction};
use super::machine_layers::LayerBuildTarget;

impl Machine<'_, '_> {
    pub(super) fn exec_race_pair(&mut self, success_only: bool, dst: usize) -> Result<(), Error> {
        let b = self.scratch.pop().expect("race b");
        let a = self.scratch.pop().expect("race a");
        self.start_race_tasks(vec![a, b], success_only, dst)
    }

    pub(super) fn exec_race_list(&mut self, success_only: bool, dst: usize) -> Result<(), Error> {
        let list_val = self.scratch.pop().expect("race tasks");
        let tasks = stdlib::runtime::coop::clone_list_values(self.heap, &list_val)?;
        self.heap.drop_value(list_val)?;
        self.start_race_tasks(tasks, success_only, dst)
    }

    fn start_race_tasks(
        &mut self,
        tasks: Vec<Value>,
        success_only: bool,
        dst: usize,
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
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target: LayerBuildTarget::Local(dst),
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

    pub(super) fn exec_timeout(
        &mut self,
        kind: TimeoutKind,
        from_duration: bool,
        dst: usize,
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
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target: LayerBuildTarget::Local(dst),
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

    pub(super) fn on_coord_race_done(
        &mut self,
        coord_id: usize,
        mut coord: CoordState,
        child_id: usize,
    ) -> Result<(), Error> {
        let done = match self.step_coord_race_mut(&mut coord, child_id) {
            Ok(done) => done,
            Err(err) => {
                self.coords.insert(coord_id, coord);
                return Err(err);
            }
        };
        if !done {
            self.coords.insert(coord_id, coord);
            return Ok(());
        }
        let LayerBuildTarget::Local(dst) = coord.target else {
            unreachable!("race target is local");
        };
        let CoordKind::Race {
            winner, failures, ..
        } = coord.kind
        else {
            unreachable!()
        };
        self.finish_coord_race(coord.parent_fiber, dst, winner, failures)
    }

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
                    let cat = primitives::concat_causes(
                        self.heap,
                        &prev,
                        &dup,
                        &mut FuelCounter(&mut self.fuel),
                    );
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
        dst: usize,
        winner: Option<FiberOutcome>,
        failures: Option<CauseToken>,
    ) -> Result<(), Error> {
        if let Some(w) = winner {
            if let Some(f) = failures {
                self.heap.drop_cause(f)?;
            }
            match w {
                FiberOutcome::Success(v) => self.wake_fiber_with_value(parent, dst, v),
                FiberOutcome::Failure(c) => self.wake_fiber_with_failure(parent, c),
                FiberOutcome::Running => self.wake_fiber_with_value(parent, dst, Value::Unit),
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

    pub(super) fn on_timeout_timer(&mut self, coord_id: usize) -> Result<(), Error> {
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
    pub(super) fn finish_coord_timeout(
        &mut self,
        parent: usize,
        dst: usize,
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
                    self.wake_fiber_with_value(parent, dst, none)
                }
                TimeoutKind::ThrowError => {
                    let err = self
                        .heap
                        .construct("Prelude.TimeoutError.TimeoutError", vec![])?;
                    let cause = self.heap.cause(&[Reason::Fail(&err)], &mut self.fuel);
                    self.heap.drop_value(err)?;
                    self.wake_fiber_with_failure(parent, cause?)
                }
                TimeoutKind::OrElse(fallback) => self.wake_fiber_run_task(parent, dst, fallback),
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
                    self.wake_fiber_with_value(parent, dst, out)
                }
                FiberOutcome::Failure(c) => {
                    let dup = self.heap.dup_cause(c)?;
                    self.wake_fiber_with_failure(parent, dup)
                }
                FiberOutcome::Running => Ok(()),
            }
        }
    }

    pub(super) fn wake_fiber_run_task(
        &mut self,
        target: usize,
        dst: usize,
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
        let res = self.await_task_reg(RegVal::Value(task), End::Store(dst));
        self.switch_to_fiber(caller);
        res?;
        self.run_queue.push_back(target);
        Ok(())
    }

    pub(super) fn interrupt_coord_parent(
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

    pub(super) fn fiber_services_for_spawn(&mut self, fid: usize) -> Result<Vec<Value>, Error> {
        if fid == self.active_fiber {
            return self.snapshot_active_services();
        }
        let caller = self.active_fiber;
        self.switch_to_fiber(fid);
        let res = self.snapshot_active_services();
        self.switch_to_fiber(caller);
        res
    }

    pub(super) fn fiber_refs_for_spawn(
        &mut self,
        fid: usize,
    ) -> Result<BTreeMap<String, Value>, Error> {
        if fid == self.active_fiber {
            return self.snapshot_fiber_refs();
        }
        let caller = self.active_fiber;
        self.switch_to_fiber(fid);
        let res = self.snapshot_fiber_refs();
        self.switch_to_fiber(caller);
        res
    }

    pub(super) fn fiber_evidence(&self, fid: usize) -> Option<OwnerEvidence> {
        if fid == self.active_fiber {
            self.current_evidence
        } else {
            self.parked_fibers
                .get(&fid)
                .and_then(|c| c.current_evidence)
        }
    }

    pub(super) fn cleanup_coords_and_groups(&mut self, roots: &mut Vec<Value>) {
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
