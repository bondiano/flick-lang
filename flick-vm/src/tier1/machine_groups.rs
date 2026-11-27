//! `FiberSet`, `FiberMap`, and `FiberHandle`: owner-hooked C1 structured fiber
//! groups for tier 1.

use flick_core::ir::memory::AsyncIntrinsic;
use std::collections::BTreeMap;

use crate::stdlib::{self, FuelCounter, primitives};
use crate::tier0::{CauseToken, Data, Error, Value};

use super::machine::{Machine, OwnerEvidence, RegVal};
use super::machine_fibers::{FiberOutcome, FiberRecord, FiberWaitKind, FiberWaiter, ParkReason};
use super::machine_scopes::OwnerHook;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FiberGroupKind {
    Set,
    Map,
    Handle,
}

pub(super) struct FiberGroupEntry {
    pub(super) fiber_id: usize,
    pub(super) key: Option<Value>,
    pub(super) propagate_interruption: bool,
}

pub(super) struct FiberGroupState {
    pub(super) kind: FiberGroupKind,
    pub(super) owner: usize,
    pub(super) closed: bool,
    pub(super) entries: BTreeMap<usize, FiberGroupEntry>,
    pub(super) next_entry: usize,
    pub(super) first_failure: Option<CauseToken>,
    pub(super) join_waiters: Vec<(usize, usize)>,
    pub(super) empty_waiters: Vec<(usize, usize)>,
}

impl Machine<'_, '_> {
    pub(super) fn fiber_group_intrinsic(
        &mut self,
        op: AsyncIntrinsic,
        dst: usize,
    ) -> Result<(), Error> {
        match op {
            AsyncIntrinsic::FiberGroupMake => {
                let Value::Int(tag) = self.scratch.pop().expect("group tag") else {
                    return Err(Error::ExpectedInt);
                };
                let kind = match tag {
                    0 => FiberGroupKind::Set,
                    1 => FiberGroupKind::Map,
                    _ => FiberGroupKind::Handle,
                };
                let owner = self.current_owner;
                let id = self.next_group;
                self.next_group = self.next_group.checked_add(1).expect("group id overflow");
                let closed = self.owners[owner].is_finalizing_or_closed();
                self.fiber_groups.insert(
                    id,
                    FiberGroupState {
                        kind,
                        owner,
                        closed,
                        entries: BTreeMap::new(),
                        next_entry: 0,
                        first_failure: None,
                        join_waiters: Vec::new(),
                        empty_waiters: Vec::new(),
                    },
                );
                if !closed {
                    self.owners[owner]
                        .hooks
                        .push_back(OwnerHook::StopFiberGroup(id));
                }
                let val = self.heap.allocate(Data::FiberGroup {
                    execution: self.execution,
                    id,
                });
                self.store_reg(dst, RegVal::Value(val))
            }
            AsyncIntrinsic::FiberGroupRun => self.exec_group_run(dst),
            AsyncIntrinsic::FiberGroupAdd => self.exec_group_add(dst),
            AsyncIntrinsic::FiberGroupGet => self.exec_group_get(dst),
            AsyncIntrinsic::FiberGroupHas => self.exec_group_has(dst),
            AsyncIntrinsic::FiberGroupRemove => self.exec_group_remove(dst),
            AsyncIntrinsic::FiberGroupClear => {
                let grp_val = self.scratch.pop().expect("group");
                let id = self.fiber_group_id_of(&grp_val)?;
                self.heap.drop_value(grp_val)?;
                self.stop_fiber_group(id, false, dst)
            }
            AsyncIntrinsic::FiberGroupSize => {
                let grp_val = self.scratch.pop().expect("group");
                let id = self.fiber_group_id_of(&grp_val)?;
                self.heap.drop_value(grp_val)?;
                let len = i32::try_from(self.fiber_groups[&id].entries.len()).unwrap_or(i32::MAX);
                self.store_reg(dst, RegVal::Value(Value::Int(len)))
            }
            AsyncIntrinsic::FiberGroupJoin => self.exec_group_join(dst),
            AsyncIntrinsic::FiberGroupAwaitEmpty => self.exec_group_await_empty(dst),
            _ => unreachable!("non-group intrinsic"),
        }
    }

    pub(super) fn fiber_group_id_of(&self, val: &Value) -> Result<usize, Error> {
        let Value::Object(tok) = val else {
            return Err(Error::ExpectedFiberGroup);
        };
        let Data::FiberGroup { execution, id } = &self.heap.object(tok.id)?.data else {
            return Err(Error::ExpectedFiberGroup);
        };
        if *execution != self.execution {
            return Err(Error::ExpectedFiberGroup);
        }
        Ok(*id)
    }

    fn make_interrupted_fiber_handle(&mut self) -> Result<Value, Error> {
        let id = self.alloc_fiber_id();
        let cause = self.interrupt_cause(self.active_fiber as u64)?;
        let mut rec = FiberRecord::root();
        rec.outcome = FiberOutcome::Failure(cause);
        self.fiber_table.insert(id, rec);
        Ok(self.heap.allocate(Data::FiberHandle {
            execution: self.execution,
            id,
        }))
    }

    pub(super) fn find_group_entry_by_key(
        &mut self,
        group_id: usize,
        key: &Value,
    ) -> Result<Option<usize>, Error> {
        let pairs: Vec<(usize, Value)> = self.fiber_groups[&group_id]
            .entries
            .iter()
            .filter_map(|(&eid, e)| e.key.as_ref().map(|k| (eid, k.representation())))
            .collect();
        for (eid, krep) in pairs {
            if primitives::values_equal(self.heap, &krep, key, &mut FuelCounter(&mut self.fuel))? {
                return Ok(Some(eid));
            }
        }
        Ok(None)
    }

    fn exec_group_run(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let task = self.scratch.pop().expect("task");
        let key_opt_val = self.scratch.pop().expect("key opt");
        let opts = self.scratch.pop().expect("opts");
        let id = self.fiber_group_id_of(&grp_val)?;
        self.heap.drop_value(grp_val)?;
        let only_if_missing =
            stdlib::runtime::coop::record_bool_field(self.heap, &opts, "onlyIfMissing");
        let prop_int =
            stdlib::runtime::coop::record_bool_field(self.heap, &opts, "propagateInterruption");
        let start_now =
            stdlib::runtime::coop::record_bool_field(self.heap, &opts, "startImmediately");
        self.heap.drop_value(opts)?;
        let key = stdlib::runtime::coop::extract_opt_value(self.heap, key_opt_val)?;

        if self.fiber_groups[&id].closed {
            self.heap.drop_value(task)?;
            if let Some(k) = key {
                self.heap.drop_value(k)?;
            }
            let dead = self.make_interrupted_fiber_handle()?;
            return self.store_reg(dst, RegVal::Value(dead));
        }

        let existing = match self.fiber_groups[&id].kind {
            FiberGroupKind::Set => None,
            FiberGroupKind::Handle => self.fiber_groups[&id].entries.keys().next().copied(),
            FiberGroupKind::Map => match &key {
                Some(k) => match self.find_group_entry_by_key(id, k) {
                    Ok(v) => v,
                    Err(err) => {
                        self.heap.drop_value(task)?;
                        if let Some(k) = key {
                            self.heap.drop_value(k)?;
                        }
                        return Err(err);
                    }
                },
                None => None,
            },
        };
        if let Some(prev_eid) = existing {
            if only_if_missing {
                let prev_fid = self.fiber_groups[&id].entries[&prev_eid].fiber_id;
                self.heap.drop_value(task)?;
                if let Some(k) = key {
                    self.heap.drop_value(k)?;
                }
                let h = self.heap.allocate(Data::FiberHandle {
                    execution: self.execution,
                    id: prev_fid,
                });
                return self.store_reg(dst, RegVal::Value(h));
            }
            if let Some(prev) = self
                .fiber_groups
                .get_mut(&id)
                .and_then(|g| g.entries.remove(&prev_eid))
            {
                if let Some(k) = prev.key {
                    self.heap.drop_value(k)?;
                }
                if let Err(err) = self.request_interrupt(prev.fiber_id, self.active_fiber as u64) {
                    self.heap.drop_value(task)?;
                    if let Some(k) = key {
                        self.heap.drop_value(k)?;
                    }
                    return Err(err);
                }
            }
        }

        let owner = self.fiber_groups[&id].owner;
        let services = self.snapshot_active_services()?;
        let refs = self.snapshot_fiber_refs()?;
        let ev = Some(OwnerEvidence::from_owner(&self.owner_handle(owner)));
        let child_id = self.spawn_raw_fiber(owner, ev, task, services, refs, 0)?;
        let eid = {
            let g = self.fiber_groups.get_mut(&id).expect("group");
            let eid = g.next_entry;
            g.next_entry += 1;
            g.entries.insert(
                eid,
                FiberGroupEntry {
                    fiber_id: child_id,
                    key,
                    propagate_interruption: prop_int,
                },
            );
            eid
        };
        if let Some(rec) = self.fiber_table.get_mut(&child_id) {
            rec.group_watchers.push((id, eid));
        }
        let handle = self.heap.allocate(Data::FiberHandle {
            execution: self.execution,
            id: child_id,
        });
        self.store_reg(dst, RegVal::Value(handle))?;
        if start_now {
            self.yield_current(Some(child_id))?;
        }
        Ok(())
    }

    fn exec_group_add(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let fib_val = self.scratch.pop().expect("fiber");
        let key_opt_val = self.scratch.pop().expect("key opt");
        let opts = self.scratch.pop().expect("opts");
        let id = self.fiber_group_id_of(&grp_val)?;
        let child_id = self.fiber_id_of(&fib_val)?;
        self.heap.drop_values(vec![grp_val, fib_val])?;
        let only_if_missing =
            stdlib::runtime::coop::record_bool_field(self.heap, &opts, "onlyIfMissing");
        let prop_int =
            stdlib::runtime::coop::record_bool_field(self.heap, &opts, "propagateInterruption");
        self.heap.drop_value(opts)?;
        let key = stdlib::runtime::coop::extract_opt_value(self.heap, key_opt_val)?;

        if self.fiber_groups[&id].closed {
            if let Some(k) = key {
                self.heap.drop_value(k)?;
            }
            self.request_interrupt(child_id, self.active_fiber as u64)?;
            return self.store_reg(dst, RegVal::Value(Value::Unit));
        }
        let existing = match self.fiber_groups[&id].kind {
            FiberGroupKind::Set => None,
            FiberGroupKind::Handle => self.fiber_groups[&id].entries.keys().next().copied(),
            FiberGroupKind::Map => match &key {
                Some(k) => match self.find_group_entry_by_key(id, k) {
                    Ok(v) => v,
                    Err(err) => {
                        if let Some(k) = key {
                            self.heap.drop_value(k)?;
                        }
                        return Err(err);
                    }
                },
                None => None,
            },
        };
        if let Some(prev_eid) = existing {
            if only_if_missing {
                if let Some(k) = key {
                    self.heap.drop_value(k)?;
                }
                return self.store_reg(dst, RegVal::Value(Value::Unit));
            }
            if let Some(prev) = self
                .fiber_groups
                .get_mut(&id)
                .and_then(|g| g.entries.remove(&prev_eid))
            {
                if let Some(k) = prev.key {
                    self.heap.drop_value(k)?;
                }
                if let Err(err) = self.request_interrupt(prev.fiber_id, self.active_fiber as u64) {
                    if let Some(k) = key {
                        self.heap.drop_value(k)?;
                    }
                    return Err(err);
                }
            }
        }
        let running = self
            .fiber_table
            .get(&child_id)
            .is_some_and(|r| matches!(r.outcome, FiberOutcome::Running));
        if running {
            let eid = {
                let g = self.fiber_groups.get_mut(&id).expect("group");
                let eid = g.next_entry;
                g.next_entry += 1;
                g.entries.insert(
                    eid,
                    FiberGroupEntry {
                        fiber_id: child_id,
                        key,
                        propagate_interruption: prop_int,
                    },
                );
                eid
            };
            if let Some(rec) = self.fiber_table.get_mut(&child_id) {
                rec.group_watchers.push((id, eid));
            }
        } else {
            if let Some(k) = key {
                self.heap.drop_value(k)?;
            }
            self.record_group_child_failure(id, child_id, prop_int)?;
        }
        self.store_reg(dst, RegVal::Value(Value::Unit))
    }

    fn exec_group_get(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let key_opt_val = self.scratch.pop().expect("key opt");
        let id = self.fiber_group_id_of(&grp_val)?;
        self.heap.drop_value(grp_val)?;
        let key = stdlib::runtime::coop::extract_opt_value(self.heap, key_opt_val)?;
        let found = match (&key, self.fiber_groups[&id].kind) {
            (Some(k), FiberGroupKind::Map) => match self.find_group_entry_by_key(id, k) {
                Ok(opt) => opt.map(|eid| self.fiber_groups[&id].entries[&eid].fiber_id),
                Err(err) => {
                    if let Some(k) = key {
                        self.heap.drop_value(k)?;
                    }
                    return Err(err);
                }
            },
            (None, FiberGroupKind::Handle) => self.fiber_groups[&id]
                .entries
                .values()
                .next()
                .map(|e| e.fiber_id),
            _ => None,
        };
        if let Some(k) = key {
            self.heap.drop_value(k)?;
        }
        let out = match found {
            Some(fid) => {
                let h = self.heap.allocate(Data::FiberHandle {
                    execution: self.execution,
                    id: fid,
                });
                self.heap.construct("Prelude.Option.Some", vec![h])?
            }
            None => self.heap.construct("Prelude.Option.None", vec![])?,
        };
        self.store_reg(dst, RegVal::Value(out))
    }

    fn exec_group_has(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let key = self.scratch.pop().expect("key");
        let id = self.fiber_group_id_of(&grp_val)?;
        self.heap.drop_value(grp_val)?;
        let res = self.find_group_entry_by_key(id, &key);
        self.heap.drop_value(key)?;
        self.store_reg(dst, RegVal::Value(Value::Bool(res?.is_some())))
    }

    fn exec_group_remove(&mut self, dst: usize) -> Result<(), Error> {
        let grp_val = self.scratch.pop().expect("group");
        let key = self.scratch.pop().expect("key");
        let id = self.fiber_group_id_of(&grp_val)?;
        self.heap.drop_value(grp_val)?;
        let found = self.find_group_entry_by_key(id, &key);
        self.heap.drop_value(key)?;
        let found = found?;
        if let Some(eid) = found
            && let Some(entry) = self
                .fiber_groups
                .get_mut(&id)
                .and_then(|g| g.entries.remove(&eid))
        {
            if let Some(k) = entry.key {
                self.heap.drop_value(k)?;
            }
            let target = entry.fiber_id;
            self.request_interrupt(target, self.active_fiber as u64)?;
            let running = self
                .fiber_table
                .get(&target)
                .is_some_and(|r| matches!(r.outcome, FiberOutcome::Running));
            if running {
                let me = self.active_fiber;
                let kind = FiberWaitKind::Interrupt(dst);
                if let Some(rec) = self.fiber_table.get_mut(&target) {
                    rec.waiters.push(FiberWaiter { fiber: me, kind });
                }
                return self.park_current(ParkReason::Fiber { target });
            }
        }
        self.store_reg(dst, RegVal::Value(Value::Unit))
    }
}
