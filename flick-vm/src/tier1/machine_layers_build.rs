//! Dynamic `Layer` build phase, `Layer.merge_all`, and `MemoMap` sharing for tier 1.

use std::collections::BTreeMap;

use flick_core::ir::memory::LayerIntrinsic;

use crate::stdlib::runtime::coop::MEMO_REF_KEY;
use crate::stdlib::runtime::layers as std_layers;
use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{CauseToken, Data, Error, LayerData, ObjectId, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_fibers::ParkReason;
use super::machine_layers::{LayerBuildTarget, MemoRestore, PendingProvide, Provider};
use super::machine_scopes::{Finalizer, OwnerState, ScopeOutcome};

pub(super) enum MemoEntryState {
    Building {
        waiters: Vec<(usize, bool, LayerBuildTarget)>,
    },
    Ready(Value),
    Failed(CauseToken),
}

pub(super) struct MemoEntry {
    pub(super) observers: usize,
    pub(super) scope_owner: usize,
    pub(super) state: MemoEntryState,
}

pub(super) struct MemoMapState {
    pub(super) parent: Option<usize>,
    pub(super) entries: BTreeMap<ObjectId, MemoEntry>,
}

enum LayerBuildKind {
    Dynamic { memo: bool, build_rep: Value },
    Fresh(Value),
    OrDie(Value),
    MergeAll(Vec<Value>),
}

impl Machine<'_, '_> {
    pub(super) fn alloc_memo_map(&mut self, parent: Option<usize>) -> usize {
        let id = self.next_memo;
        self.next_memo = self.next_memo.checked_add(1).expect("memo id overflow");
        self.memo_maps.insert(
            id,
            MemoMapState {
                parent,
                entries: BTreeMap::new(),
            },
        );
        id
    }

    fn memo_map_id_of(&self, val: &Value) -> Result<usize, Error> {
        let Value::Object(tok) = val else {
            return Err(Error::ExpectedMemoMap);
        };
        let Data::MemoMap { execution, id } = &self.heap.object(tok.id)?.data else {
            return Err(Error::ExpectedMemoMap);
        };
        if *execution != self.execution {
            return Err(Error::ExpectedMemoMap);
        }
        Ok(*id)
    }

    fn current_or_forked_memo_id(&mut self) -> Result<usize, Error> {
        let parent = match self.fiber_refs.get(MEMO_REF_KEY) {
            Some(v) => Some(self.memo_map_id_of(v)?),
            None => None,
        };
        Ok(self.alloc_memo_map(parent))
    }

    fn set_fiber_memo(&mut self, memo_id: usize) -> Option<Value> {
        let val = self.heap.allocate(Data::MemoMap {
            execution: self.execution,
            id: memo_id,
        });
        self.fiber_refs.insert(MEMO_REF_KEY.to_owned(), val)
    }

    pub(super) fn restore_fiber_memo(&mut self, prev: Option<Value>) -> Result<(), Error> {
        let replaced = match prev {
            Some(v) => self.fiber_refs.insert(MEMO_REF_KEY.to_owned(), v),
            None => self.fiber_refs.remove(MEMO_REF_KEY),
        };
        if let Some(old) = replaced {
            self.heap.drop_value(old)?;
        }
        Ok(())
    }

    pub(super) fn layer_memo_intrinsic(
        &mut self,
        op: LayerIntrinsic,
        dst: usize,
    ) -> Result<(), Error> {
        match op {
            LayerIntrinsic::LayerMakeMemoMap => {
                self.release_operands()?;
                let id = self.alloc_memo_map(None);
                let val = self.heap.allocate(Data::MemoMap {
                    execution: self.execution,
                    id,
                });
                self.store_reg(dst, RegVal::Value(val))
            }
            LayerIntrinsic::LayerForkMemoMap => {
                let parent_val = self.scratch.pop().expect("parent memo");
                let parent_id = self.memo_map_id_of(&parent_val)?;
                self.heap.drop_value(parent_val)?;
                let id = self.alloc_memo_map(Some(parent_id));
                let val = self.heap.allocate(Data::MemoMap {
                    execution: self.execution,
                    id,
                });
                self.store_reg(dst, RegVal::Value(val))
            }
            LayerIntrinsic::LayerCurrentMemoMap => {
                self.release_operands()?;
                let id = self.current_or_forked_memo_id()?;
                let val = self.heap.allocate(Data::MemoMap {
                    execution: self.execution,
                    id,
                });
                self.store_reg(dst, RegVal::Value(val))
            }
            LayerIntrinsic::LayerBuild => {
                let layer = self.scratch.pop().expect("layer");
                let memo_id = self.current_or_forked_memo_id()?;
                let owner = self.current_owner;
                self.begin_build_layer(layer, memo_id, owner, false, LayerBuildTarget::Local(dst))
            }
            LayerIntrinsic::LayerBuildWithMemoMap => {
                let layer = self.scratch.pop().expect("layer");
                let memo_val = self.scratch.pop().expect("memo");
                let memo_id = self.memo_map_id_of(&memo_val)?;
                self.heap.drop_value(memo_val)?;
                let owner = self.current_owner;
                self.begin_build_layer(layer, memo_id, owner, false, LayerBuildTarget::Local(dst))
            }
            _ => unreachable!("non-memo layer intrinsic"),
        }
    }

    pub(super) fn begin_dynamic_layer_run(
        &mut self,
        layer: Value,
        body_thunk: Value,
        dst: usize,
    ) -> Result<(), Error> {
        let previous = self.current_owner;
        let saved_evidence = self.current_evidence;
        let owner = self.owners.len();
        self.owners.push(OwnerState::default());
        let memo_id = self.current_or_forked_memo_id()?;
        let prev_memo = self.set_fiber_memo(memo_id);
        self.current_owner = owner;
        self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(owner)));
        let pid = self.next_provide;
        self.next_provide = self
            .next_provide
            .checked_add(1)
            .expect("provide id overflow");
        self.pending_provides.insert(
            pid,
            PendingProvide {
                owner,
                previous,
                body_thunk,
                saved_evidence,
                prev_memo,
                dst,
            },
        );
        self.begin_build_layer(layer, memo_id, owner, false, LayerBuildTarget::Provide(pid))
    }

    pub(super) fn deliver_layer_build_ok(
        &mut self,
        target: LayerBuildTarget,
        ctx: Value,
    ) -> Result<(), Error> {
        match target {
            LayerBuildTarget::Local(dst) => self.store_reg(dst, RegVal::Value(ctx)),
            LayerBuildTarget::Top => {
                self.top = Some(RegVal::Value(ctx));
                Ok(())
            }
            LayerBuildTarget::Provide(pid) => self.complete_dynamic_provide_ok(pid, ctx),
        }
    }

    pub(super) fn deliver_layer_build_err(
        &mut self,
        target: LayerBuildTarget,
        cause: CauseToken,
    ) -> Result<(), Error> {
        match target {
            LayerBuildTarget::Local(_) | LayerBuildTarget::Top => {
                self.failure = Some(cause);
                self.failure_elected = true;
                Ok(())
            }
            LayerBuildTarget::Provide(pid) => self.complete_dynamic_provide_err(pid, cause),
        }
    }

    pub(super) fn complete_dynamic_provide_ok(
        &mut self,
        pid: usize,
        built_ctx: Value,
    ) -> Result<(), Error> {
        let pp = self
            .pending_provides
            .remove(&pid)
            .expect("pending dynamic provide");
        let id = self.handler_identity()?;
        self.providers.insert(
            id,
            Provider {
                layer: built_ctx,
                owner: pp.owner,
                previous: pp.previous,
                restore_memo: MemoRestore::Restore(pp.prev_memo),
            },
        );
        self.current_owner = pp.owner;
        self.current_evidence = pp.saved_evidence;
        self.bump_handler_epoch();
        self.apply_reg(
            RegVal::Value(pp.body_thunk),
            RegVal::Value(Value::Unit),
            End::LayerRun {
                provider: id,
                dst: pp.dst,
            },
            false,
        )
    }

    pub(super) fn complete_dynamic_provide_err(
        &mut self,
        pid: usize,
        cause: CauseToken,
    ) -> Result<(), Error> {
        let pp = self
            .pending_provides
            .remove(&pid)
            .expect("pending dynamic provide");
        self.heap.drop_value(pp.body_thunk)?;
        self.restore_fiber_memo(pp.prev_memo)?;
        self.current_evidence = pp.saved_evidence;
        self.begin_close_scope_elect(pp.owner, pp.previous, ScopeOutcome::Failure(cause), true)
    }

    pub(super) fn begin_build_layer(
        &mut self,
        layer: Value,
        memo_id: usize,
        scope_owner: usize,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        if std_layers::layer_is_static(self.heap, &layer)? {
            let ctx = std_layers::eval_static_layer_to_context(self.heap, layer)?;
            return self.deliver_layer_build_ok(target, ctx);
        }
        let Value::Object(tok) = &layer else {
            self.heap.drop_value(layer)?;
            return Err(Error::ExpectedLayer);
        };
        let layer_id = tok.id;
        let kind = match &self.heap.object(layer_id)?.data {
            Data::Layer(LayerData::Dynamic { memo, build }) => LayerBuildKind::Dynamic {
                memo: *memo,
                build_rep: build.representation(),
            },
            Data::Layer(LayerData::Fresh(inner)) => LayerBuildKind::Fresh(inner.representation()),
            Data::Layer(LayerData::OrDie(inner)) => LayerBuildKind::OrDie(inner.representation()),
            Data::Layer(LayerData::Merge { left, right }) => {
                LayerBuildKind::MergeAll(vec![left.representation(), right.representation()])
            }
            Data::Layer(LayerData::MergeAll(ls)) => {
                LayerBuildKind::MergeAll(ls.iter().map(Value::representation).collect())
            }
            _ => {
                self.heap.drop_value(layer)?;
                return Err(Error::ExpectedLayer);
            }
        };
        match kind {
            LayerBuildKind::Fresh(inner_rep) => {
                let inner = self.heap.dup(&inner_rep)?;
                self.heap.drop_value(layer)?;
                let fresh_memo = self.alloc_memo_map(None);
                self.begin_build_layer(inner, fresh_memo, scope_owner, or_die, target)
            }
            LayerBuildKind::OrDie(inner_rep) => {
                let inner = self.heap.dup(&inner_rep)?;
                self.heap.drop_value(layer)?;
                self.begin_build_layer(inner, memo_id, scope_owner, true, target)
            }
            LayerBuildKind::MergeAll(reps) => {
                let mut layers = Vec::with_capacity(reps.len());
                for r in reps {
                    layers.push(self.heap.dup(&r)?);
                }
                self.heap.drop_value(layer)?;
                self.start_layer_merge_all(layers, memo_id, scope_owner, or_die, target)
            }
            LayerBuildKind::Dynamic { memo, build_rep } => {
                let build_fn = self.heap.dup(&build_rep)?;
                self.heap.drop_value(layer)?;
                if memo {
                    self.build_memoized_layer(
                        layer_id,
                        build_fn,
                        memo_id,
                        scope_owner,
                        or_die,
                        target,
                    )
                } else {
                    self.build_unmemoized_layer(build_fn, memo_id, scope_owner, or_die, target)
                }
            }
        }
    }

    fn build_unmemoized_layer(
        &mut self,
        build_fn: Value,
        memo_id: usize,
        parent_scope: usize,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        let inner_owner = self.owners.len();
        self.owners.push(OwnerState::default());
        let prev_owner = self.current_owner;
        let prev_ev = self.current_evidence;
        let prev_memo = self.set_fiber_memo(memo_id);
        let memo_arg = self.heap.allocate(Data::MemoMap {
            execution: self.execution,
            id: memo_id,
        });
        self.current_owner = inner_owner;
        self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(inner_owner)));
        self.apply_reg(
            RegVal::Value(build_fn),
            RegVal::Value(memo_arg),
            End::LayerBuildFinish {
                inner_owner,
                parent_scope,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry: None,
                or_die,
                target,
            },
            false,
        )
    }

    fn lookup_memo_entry(&self, memo_id: usize, layer_id: ObjectId) -> Option<usize> {
        let mut cur = Some(memo_id);
        while let Some(mid) = cur {
            let state = self.memo_maps.get(&mid)?;
            if state.entries.contains_key(&layer_id) {
                return Some(mid);
            }
            cur = state.parent;
        }
        None
    }

    #[allow(clippy::too_many_lines)]
    fn build_memoized_layer(
        &mut self,
        layer_id: ObjectId,
        build_fn: Value,
        memo_id: usize,
        parent_scope: usize,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        if let Some(holder_id) = self.lookup_memo_entry(memo_id, layer_id) {
            self.heap.drop_value(build_fn)?;
            if !self.owners[parent_scope].is_finalizing_or_closed() {
                let entry = self
                    .memo_maps
                    .get_mut(&holder_id)
                    .expect("memo")
                    .entries
                    .get_mut(&layer_id)
                    .expect("entry");
                entry.observers += 1;
                let entry_owner = entry.scope_owner;
                self.owners[parent_scope]
                    .finalizers
                    .push(Finalizer::ReleaseMemo {
                        memo_id: holder_id,
                        layer_id,
                        entry_owner,
                    });
            }
            match &self.memo_maps[&holder_id].entries[&layer_id].state {
                MemoEntryState::Ready(ctx) => {
                    let rep = ctx.representation();
                    let dup = self.heap.dup(&rep)?;
                    return self.deliver_layer_build_ok(target, dup);
                }
                MemoEntryState::Failed(c) => {
                    let mut cause = self.heap.dup_cause(c)?;
                    if or_die {
                        let conv = primitives::fails_to_dies_cause(
                            self.heap,
                            &cause,
                            &mut FuelCounter(&mut self.fuel),
                        );
                        self.heap.drop_cause(cause)?;
                        cause = conv?;
                    }
                    return self.deliver_layer_build_err(target, cause);
                }
                MemoEntryState::Building { .. } => {
                    let me = self.active_fiber;
                    if let MemoEntryState::Building { waiters } = &mut self
                        .memo_maps
                        .get_mut(&holder_id)
                        .expect("memo")
                        .entries
                        .get_mut(&layer_id)
                        .expect("entry")
                        .state
                    {
                        waiters.push((me, or_die, target));
                    }
                    return self.park_current(ParkReason::MemoWait { target });
                }
            }
        }
        if self.owners[parent_scope].is_finalizing_or_closed() {
            return self.build_unmemoized_layer(build_fn, memo_id, parent_scope, or_die, target);
        }
        let entry_owner = self.owners.len();
        self.owners.push(OwnerState::default());
        self.memo_maps
            .get_mut(&memo_id)
            .expect("memo")
            .entries
            .insert(
                layer_id,
                MemoEntry {
                    observers: 1,
                    scope_owner: entry_owner,
                    state: MemoEntryState::Building {
                        waiters: Vec::new(),
                    },
                },
            );
        self.owners[parent_scope]
            .finalizers
            .push(Finalizer::ReleaseMemo {
                memo_id,
                layer_id,
                entry_owner,
            });
        let prev_owner = self.current_owner;
        let prev_ev = self.current_evidence;
        let prev_memo = self.set_fiber_memo(memo_id);
        let memo_arg = self.heap.allocate(Data::MemoMap {
            execution: self.execution,
            id: memo_id,
        });
        self.current_owner = entry_owner;
        self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(entry_owner)));
        self.apply_reg(
            RegVal::Value(build_fn),
            RegVal::Value(memo_arg),
            End::LayerBuildFinish {
                inner_owner: entry_owner,
                parent_scope,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry: Some((memo_id, layer_id)),
                or_die,
                target,
            },
            false,
        )
    }
}
