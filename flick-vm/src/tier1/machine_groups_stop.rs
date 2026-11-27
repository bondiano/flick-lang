//! `FiberGroup` shutdown, joining, await-empty, and child-completion handling for tier 1.

use std::mem;

use crate::tier0::causes::StoredReason;
use crate::tier0::{Error, Value};

use super::machine::{Machine, RegVal};
use super::machine_coords::{CoordKind, CoordState};
use super::machine_fibers::{FiberOutcome, FiberWaitKind, FiberWaiter, ParkReason};
use super::machine_layers::LayerBuildTarget;

impl Machine<'_, '_> {
    pub(super) fn drain_group_for_stop(
        &mut self,
        id: usize,
        mark_closed: bool,
    ) -> Result<Vec<usize>, Error> {
        let Some(g) = self.fiber_groups.get_mut(&id) else {
            return Ok(Vec::new());
        };
        if mark_closed {
            g.closed = true;
        }
        let entries = mem::take(&mut g.entries);
        let join_waiters = if mark_closed {
            mem::take(&mut g.join_waiters)
        } else {
            Vec::new()
        };
        let empty_waiters = mem::take(&mut g.empty_waiters);
        let mut fids = Vec::with_capacity(entries.len());
        let mut keys = Vec::new();
        for entry in entries.into_values() {
            fids.push(entry.fiber_id);
            if let Some(k) = entry.key {
                keys.push(k);
            }
        }
        self.heap.drop_values(keys)?;
        let me = self.active_fiber;
        for &fid in &fids {
            self.request_interrupt(fid, me as u64)?;
        }
        for (wf, wd) in join_waiters {
            self.wake_fiber_with_value(wf, wd, Value::Unit)?;
        }
        for (wf, wd) in empty_waiters {
            self.wake_fiber_with_value(wf, wd, Value::Unit)?;
        }
        Ok(fids)
    }

    pub(super) fn stop_fiber_group(
        &mut self,
        id: usize,
        mark_closed: bool,
        dst: usize,
    ) -> Result<(), Error> {
        let fids = self.drain_group_for_stop(id, mark_closed)?;
        let coord_id = self.alloc_coord_id();
        let me = self.active_fiber;
        let mut pending = 0;
        for (idx, fid) in fids.into_iter().enumerate() {
            if self
                .fiber_table
                .get(&fid)
                .is_some_and(|r| matches!(r.outcome, FiberOutcome::Running))
            {
                if let Some(rec) = self.fiber_table.get_mut(&fid) {
                    rec.waiters.push(FiberWaiter {
                        fiber: me,
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
                parent_fiber: me,
                target: LayerBuildTarget::Local(dst),
                kind: CoordKind::WaitAll {
                    pending,
                    done_value: Value::Unit,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    pub(super) fn stop_fiber_group_for_close(
        &mut self,
        group_id: usize,
        owner: usize,
    ) -> Result<Option<usize>, Error> {
        let fids = self.drain_group_for_stop(group_id, true)?;
        let coord_id = self.alloc_coord_id();
        let me = self.active_fiber;
        let mut pending = 0;
        for (idx, fid) in fids.into_iter().enumerate() {
            if self
                .fiber_table
                .get(&fid)
                .is_some_and(|r| matches!(r.outcome, FiberOutcome::Running))
            {
                if let Some(rec) = self.fiber_table.get_mut(&fid) {
                    rec.waiters.push(FiberWaiter {
                        fiber: me,
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
            return Ok(None);
        }
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: me,
                target: LayerBuildTarget::Top,
                kind: CoordKind::OwnerHookWait { owner, pending },
            },
        );
        Ok(Some(coord_id))
    }

    pub(super) fn exec_group_join(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let id = self.fiber_group_id_of(&grp_val)?;
        self.heap.drop_value(grp_val)?;
        if let Some(c) = &self.fiber_groups[&id].first_failure {
            self.failure = Some(self.heap.dup_cause(c)?);
            self.failure_elected = true;
            return Ok(());
        }
        if self.fiber_groups[&id].closed {
            return self.store_reg(dst, RegVal::Value(Value::Unit));
        }
        let me = self.active_fiber;
        self.fiber_groups
            .get_mut(&id)
            .expect("group")
            .join_waiters
            .push((me, dst));
        self.park_current(ParkReason::FiberGroupJoin)
    }

    pub(super) fn exec_group_await_empty(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let id = self.fiber_group_id_of(&grp_val)?;
        self.heap.drop_value(grp_val)?;
        if self.fiber_groups[&id].entries.is_empty() {
            return self.store_reg(dst, RegVal::Value(Value::Unit));
        }
        let me = self.active_fiber;
        self.fiber_groups
            .get_mut(&id)
            .expect("group")
            .empty_waiters
            .push((me, dst));
        self.park_current(ParkReason::FiberGroupEmpty)
    }

    pub(super) fn on_group_child_done(
        &mut self,
        group_id: usize,
        entry_id: usize,
        child_id: usize,
    ) -> Result<(), Error> {
        let removed = self
            .fiber_groups
            .get_mut(&group_id)
            .and_then(|g| g.entries.remove(&entry_id));
        let prop_int = if let Some(entry) = removed {
            if let Some(k) = entry.key {
                self.heap.drop_value(k)?;
            }
            entry.propagate_interruption
        } else {
            return Ok(());
        };
        self.record_group_child_failure(group_id, child_id, prop_int)?;
        if self
            .fiber_groups
            .get(&group_id)
            .is_some_and(|g| g.entries.is_empty())
        {
            let waiters = mem::take(
                &mut self
                    .fiber_groups
                    .get_mut(&group_id)
                    .expect("group")
                    .empty_waiters,
            );
            for (wf, wd) in waiters {
                self.wake_fiber_with_value(wf, wd, Value::Unit)?;
            }
        }
        Ok(())
    }

    pub(super) fn record_group_child_failure(
        &mut self,
        group_id: usize,
        child_id: usize,
        prop_int: bool,
    ) -> Result<(), Error> {
        let FiberOutcome::Failure(c) = &self.fiber_table[&child_id].outcome else {
            return Ok(());
        };
        let int_only = {
            let rs = self.heap.cause_data(c)?;
            !rs.is_empty()
                && rs
                    .iter()
                    .all(|r| matches!(r, StoredReason::Interrupt { .. }))
        };
        if int_only && !prop_int {
            return Ok(());
        }
        if self.fiber_groups[&group_id].first_failure.is_some() {
            return Ok(());
        }
        let dup = self.heap.dup_cause(c)?;
        let waiters = {
            let g = self.fiber_groups.get_mut(&group_id).expect("group");
            g.first_failure = Some(dup);
            mem::take(&mut g.join_waiters)
        };
        for (wf, _wd) in waiters {
            let c_dup = self.heap.dup_cause(
                self.fiber_groups[&group_id]
                    .first_failure
                    .as_ref()
                    .expect("fail"),
            )?;
            self.wake_fiber_with_failure(wf, c_dup)?;
        }
        Ok(())
    }
}
