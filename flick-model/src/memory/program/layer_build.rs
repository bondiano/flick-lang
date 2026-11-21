// Dynamic `Layer` build phase and `MemoMap` sharing (Stage 4A).

const MEMO_REF_KEY: &str = "effect/Layer/CurrentMemoMap";

enum MemoEntryState {
    Building {
        waiters: Vec<(usize, bool, LayerBuildTarget)>,
    },
    Ready(Value),
    Failed(CauseToken),
}

struct MemoEntry {
    observers: usize,
    scope_owner: usize,
    state: MemoEntryState,
}

struct MemoMapState {
    parent: Option<usize>,
    entries: BTreeMap<ObjectId, MemoEntry>,
}

enum LayerBuildKind {
    Dynamic { memo: bool, build_rep: Value },
    Fresh(Value),
    OrDie(Value),
    MergeAll(Vec<Value>),
}

impl Machine<'_, '_> {
    fn alloc_memo_map(&mut self, parent: Option<usize>) -> usize {
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

    pub(super) fn layer_is_static(&self, layer: &Value) -> Result<bool, Error> {
        let mut pending = vec![layer];
        while let Some(v) = pending.pop() {
            let Value::Object(tok) = v else {
                return Err(Error::ExpectedLayer);
            };
            let Data::Layer(data) = &self.heap.object(tok.id)?.data else {
                return Err(Error::ExpectedLayer);
            };
            match data {
                LayerData::Succeed { .. } | LayerData::Empty | LayerData::Context(_) => {}
                LayerData::Merge { left, right } => pending.extend([left, right]),
                LayerData::Dynamic { .. }
                | LayerData::Fresh(_)
                | LayerData::MergeAll(_)
                | LayerData::OrDie(_) => return Ok(false),
            }
        }
        Ok(true)
    }

    fn eval_static_layer_to_context(&mut self, layer: Value) -> Result<Value, Error> {
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        let mut stack = vec![layer];
        while let Some(v) = stack.pop() {
            let Value::Object(tok) = &v else {
                self.heap.drop_value(v)?;
                self.heap.drop_values(stack)?;
                self.heap.drop_values(map.into_values().collect())?;
                return Err(Error::ExpectedLayer);
            };
            let action = match &self.heap.object(tok.id)?.data {
                Data::Layer(LayerData::Empty) => 0,
                Data::Layer(LayerData::Succeed { .. }) => 1,
                Data::Layer(LayerData::Context(_)) => 2,
                Data::Layer(LayerData::Merge { .. }) => 3,
                _ => {
                    self.heap.drop_value(v)?;
                    self.heap.drop_values(stack)?;
                    self.heap.drop_values(map.into_values().collect())?;
                    return Err(Error::ExpectedLayer);
                }
            };
            match action {
                0 => self.heap.drop_value(v)?,
                1 => {
                    let (key, imp_rep) = {
                        let Data::Layer(LayerData::Succeed {
                            tag,
                            implementation,
                        }) = &self.heap.object(tok.id)?.data
                        else {
                            unreachable!()
                        };
                        (
                            self.service_tag_key(tag)?.to_owned(),
                            implementation.representation(),
                        )
                    };
                    let imp = self.heap.dup(&imp_rep)?;
                    self.heap.drop_value(v)?;
                    if let Some(old) = map.insert(key, imp) {
                        self.heap.drop_value(old)?;
                    }
                }
                2 => {
                    let ctx_rep = {
                        let Data::Layer(LayerData::Context(c)) = &self.heap.object(tok.id)?.data
                        else {
                            unreachable!()
                        };
                        c.representation()
                    };
                    let ctx_map = self.clone_context_map(&ctx_rep)?;
                    self.heap.drop_value(v)?;
                    for (k, imp) in ctx_map {
                        if let Some(old) = map.insert(k, imp) {
                            self.heap.drop_value(old)?;
                        }
                    }
                }
                3 => {
                    let (l_rep, r_rep) = {
                        let Data::Layer(LayerData::Merge { left, right }) =
                            &self.heap.object(tok.id)?.data
                        else {
                            unreachable!()
                        };
                        (left.representation(), right.representation())
                    };
                    let right = self.heap.dup(&r_rep)?;
                    let left = self.heap.dup(&l_rep)?;
                    self.heap.drop_value(v)?;
                    stack.push(right);
                    stack.push(left);
                }
                _ => unreachable!(),
            }
        }
        Ok(self.heap.allocate(Data::Context(map)))
    }

    pub(super) fn begin_dynamic_layer_run(
        &mut self,
        layer: Value,
        body_thunk: Value,
        binding: Local,
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
                binding,
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
            LayerBuildTarget::Local(binding) => self.store(binding, ctx),
            LayerBuildTarget::Top => {
                self.top = Some(ctx);
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

    fn complete_dynamic_provide_ok(&mut self, pid: usize, built_ctx: Value) -> Result<(), Error> {
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
        self.scratch.extend([pp.body_thunk, Value::Unit]);
        self.apply_fresh(Destination::LayerRun {
            provider: id,
            binding: pp.binding,
        })
    }

    fn complete_dynamic_provide_err(&mut self, pid: usize, cause: CauseToken) -> Result<(), Error> {
        let pp = self
            .pending_provides
            .remove(&pid)
            .expect("pending dynamic provide");
        self.heap.drop_value(pp.body_thunk)?;
        self.restore_fiber_memo(pp.prev_memo)?;
        self.current_evidence = pp.saved_evidence;
        self.begin_close_scope_elect(pp.owner, pp.previous, ScopeOutcome::Failure(cause), true)
    }

    fn begin_build_layer(
        &mut self,
        layer: Value,
        memo_id: usize,
        scope_owner: usize,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        if self.layer_is_static(&layer)? {
            let ctx = self.eval_static_layer_to_context(layer)?;
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
            _ => unreachable!("static layers handled above"),
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
        self.scratch.extend([build_fn, memo_arg]);
        self.apply_fresh(Destination::LayerBuildFinish {
            inner_owner,
            parent_scope,
            prev_owner,
            prev_ev,
            prev_memo,
            memo_entry: None,
            or_die,
            target,
        })
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
                self.memo_maps
                    .get_mut(&holder_id)
                    .expect("memo")
                    .entries
                    .get_mut(&layer_id)
                    .expect("entry")
                    .observers += 1;
                self.owners[parent_scope]
                    .finalizers
                    .push(Finalizer::ReleaseMemo {
                        memo_id: holder_id,
                        layer_id,
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
                        let conv =
                            primitives::fails_to_dies_cause(self.heap, &cause, &mut self.fuel);
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
            .push(Finalizer::ReleaseMemo { memo_id, layer_id });
        let prev_owner = self.current_owner;
        let prev_ev = self.current_evidence;
        let prev_memo = self.set_fiber_memo(memo_id);
        let memo_arg = self.heap.allocate(Data::MemoMap {
            execution: self.execution,
            id: memo_id,
        });
        self.current_owner = entry_owner;
        self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(entry_owner)));
        self.scratch.extend([build_fn, memo_arg]);
        self.apply_fresh(Destination::LayerBuildFinish {
            inner_owner: entry_owner,
            parent_scope,
            prev_owner,
            prev_ev,
            prev_memo,
            memo_entry: Some((memo_id, layer_id)),
            or_die,
            target,
        })
    }
}
