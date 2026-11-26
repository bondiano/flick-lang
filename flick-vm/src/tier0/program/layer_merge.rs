// Layer merge_all, build completion, service lookup, and memo cleanup (Stage 4A).

impl<'p> Machine<'p, '_> {
    fn start_layer_merge_all(
        &mut self,
        layers: Vec<Value>,
        memo_id: usize,
        parent_owner: usize,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        let total = layers.len();
        let mut scopes = Vec::with_capacity(total);
        for _ in 0..total {
            let s = self.owners.len();
            self.owners.push(OwnerState::default());
            scopes.push(s);
        }
        let coord_id = self.alloc_coord_id();
        let parent = self.active_fiber;
        let mut children = Vec::with_capacity(total);
        for (idx, (l, sub_owner)) in layers.into_iter().zip(scopes.iter().copied()).enumerate() {
            let services = self.snapshot_active_services()?;
            let mut refs = self.snapshot_fiber_refs()?;
            let memo_val = self.heap.allocate(Data::MemoMap {
                execution: self.execution,
                id: memo_id,
            });
            if let Some(old) = refs.insert(MEMO_REF_KEY.to_owned(), memo_val) {
                self.heap.drop_value(old)?;
            }
            let child_id = self.alloc_fiber_id();
            let ev = Some(OwnerEvidence::from_owner(&self.owner_handle(sub_owner)));
            let mut ctx = FiberContext::empty(sub_owner, ev);
            ctx.inherited_services = services;
            ctx.fiber_refs = refs;
            ctx.seeded_random = self.seeded_random;
            ctx.pending_layer_build = Some((l, memo_id, sub_owner));
            self.parked_fibers.insert(child_id, ctx);
            let mut rec = FiberRecord::root();
            rec.coord_parent = Some((coord_id, idx));
            self.fiber_table.insert(child_id, rec);
            self.run_queue.push_back(child_id);
            children.push(child_id);
        }
        self.coords.insert(
            coord_id,
            CoordState {
                parent_fiber: parent,
                target,
                kind: CoordKind::All {
                    mode: AllMode::MergeLayers {
                        scopes,
                        parent_owner,
                        or_die,
                    },
                    queued: VecDeque::new(),
                    children,
                    results: (0..total).map(|_| None).collect(),
                    first_failure: None,
                    stopping: false,
                    pending: total,
                    owner: parent_owner,
                },
            },
        );
        self.park_current(ParkReason::Coord { coord_id })
    }

    #[expect(clippy::too_many_arguments, reason = "layer build completion context")]
    pub(super) fn complete_layer_build_ok(
        &mut self,
        inner_owner: usize,
        parent_scope: usize,
        prev_owner: usize,
        prev_ev: Option<OwnerEvidence>,
        prev_memo: Option<Value>,
        memo_entry: Option<(usize, ObjectId)>,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        let ctx = self.pending.take().expect("built context");
        self.current_owner = prev_owner;
        self.current_evidence = prev_ev;
        self.restore_fiber_memo(prev_memo)?;
        if let Some((mid, lid)) = memo_entry {
            let cached = self.heap.dup(&ctx)?;
            let waiters = if let Some(entry) = self
                .memo_maps
                .get_mut(&mid)
                .and_then(|m| m.entries.get_mut(&lid))
                .filter(|e| e.scope_owner == inner_owner)
            {
                let old = mem::replace(&mut entry.state, MemoEntryState::Ready(cached));
                if let MemoEntryState::Building { waiters } = old {
                    waiters
                } else {
                    Vec::new()
                }
            } else {
                self.heap.drop_value(cached)?;
                Vec::new()
            };
            for (wf, _or_die, wt) in waiters {
                let dup = self.heap.dup(&ctx)?;
                self.wake_fiber_build_value(wf, wt, dup)?;
            }
        } else {
            self.owners[parent_scope]
                .finalizers
                .push(Finalizer::CloseChildOwner(inner_owner));
        }
        self.deliver_layer_build_ok(target, ctx)
    }

    #[expect(clippy::too_many_arguments, reason = "layer build error context")]
    pub(super) fn complete_layer_build_err(
        &mut self,
        inner_owner: usize,
        prev_owner: usize,
        prev_ev: Option<OwnerEvidence>,
        prev_memo: Option<Value>,
        memo_entry: Option<(usize, ObjectId)>,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        self.current_owner = prev_owner;
        self.current_evidence = prev_ev;
        self.restore_fiber_memo(prev_memo)?;
        let mut cause = self.failure.take().expect("layer build failure");
        if let Some((mid, lid)) = memo_entry {
            let cached = self.heap.dup_cause(&cause)?;
            let waiters = if let Some(entry) = self
                .memo_maps
                .get_mut(&mid)
                .and_then(|m| m.entries.get_mut(&lid))
                .filter(|e| e.scope_owner == inner_owner)
            {
                let old = mem::replace(&mut entry.state, MemoEntryState::Failed(cached));
                if let MemoEntryState::Building { waiters } = old {
                    waiters
                } else {
                    Vec::new()
                }
            } else {
                self.heap.drop_cause(cached)?;
                Vec::new()
            };
            for (wf, w_or_die, wt) in waiters {
                let mut dup = self.heap.dup_cause(&cause)?;
                if w_or_die {
                    let conv = primitives::fails_to_dies_cause(self.heap, &dup, &mut FuelCounter(&mut self.fuel));
                    self.heap.drop_cause(dup)?;
                    dup = match conv {
                        Ok(c) => c,
                        Err(err) => {
                            self.heap.drop_cause(cause)?;
                            return Err(err);
                        }
                    };
                }
                self.wake_fiber_build_failure(wf, wt, dup)?;
            }
        }
        if or_die {
            let conv = primitives::fails_to_dies_cause(self.heap, &cause, &mut FuelCounter(&mut self.fuel));
            self.heap.drop_cause(cause)?;
            cause = conv?;
        }
        // If this failed build was for a dynamic `provide`, clean up its `PendingProvide`
        // after `inner_owner` closes (combining with defect election).
        if let LayerBuildTarget::Provide(pid) = target
            && let Some(pp) = self.pending_provides.remove(&pid)
        {
            self.heap.drop_value(pp.body_thunk)?;
            self.restore_fiber_memo(pp.prev_memo)?;
            self.current_evidence = pp.saved_evidence;
            // Attach inner_owner to pp.owner so closing pp.owner closes inner_owner too.
            self.owners[pp.owner]
                .finalizers
                .push(Finalizer::CloseChildOwner(inner_owner));
            return self.begin_close_scope_elect(
                pp.owner,
                pp.previous,
                ScopeOutcome::Failure(cause),
                true,
            );
        }
        self.begin_close_scope_elect(inner_owner, prev_owner, ScopeOutcome::Failure(cause), true)
    }

    #[expect(clippy::too_many_arguments, reason = "layer build discard context")]
    pub(super) fn discard_layer_build_finish(
        &mut self,
        inner_owner: usize,
        prev_owner: usize,
        prev_ev: Option<OwnerEvidence>,
        prev_memo: Option<Value>,
        memo_entry: Option<(usize, ObjectId)>,
        target: LayerBuildTarget,
        roots: Vec<Value>,
    ) -> Result<(), Error> {
        self.current_owner = prev_owner;
        self.current_evidence = prev_ev;
        self.restore_fiber_memo(prev_memo)?;
        self.heap.drop_values(roots)?;
        if let Some((mid, lid)) = memo_entry
            && let Some(memo) = self.memo_maps.get_mut(&mid)
            && memo
                .entries
                .get(&lid)
                .is_some_and(|e| e.scope_owner == inner_owner)
            && let Some(entry) = memo.entries.remove(&lid)
        {
            match entry.state {
                MemoEntryState::Building { waiters } => {
                    if !waiters.is_empty() {
                        let base = self.interrupt_cause(self.active_fiber as u64)?;
                        for (wf, w_or_die, wt) in waiters {
                            let mut dup = self.heap.dup_cause(&base)?;
                            if w_or_die {
                                let conv = primitives::fails_to_dies_cause(
                                    self.heap,
                                    &dup,
                                    &mut FuelCounter(&mut self.fuel),
                                );
                                self.heap.drop_cause(dup)?;
                                dup = match conv {
                                    Ok(c) => c,
                                    Err(err) => {
                                        self.heap.drop_cause(base)?;
                                        return Err(err);
                                    }
                                };
                            }
                            self.wake_fiber_build_failure(wf, wt, dup)?;
                        }
                        self.heap.drop_cause(base)?;
                    }
                }
                MemoEntryState::Ready(v) => self.heap.drop_value(v)?,
                MemoEntryState::Failed(c) => self.heap.drop_cause(c)?,
            }
        }
        if let LayerBuildTarget::Provide(pid) = target
            && let Some(pp) = self.pending_provides.remove(&pid)
        {
            self.heap.drop_value(pp.body_thunk)?;
            self.restore_fiber_memo(pp.prev_memo)?;
            self.current_evidence = pp.saved_evidence;
            if !self.owners[inner_owner].is_closed() {
                self.owners[pp.owner]
                    .finalizers
                    .push(Finalizer::CloseChildOwner(inner_owner));
            }
            if self.owner_needs_async_close(pp.owner) {
                let paused = self.discard.take().expect("active transfer");
                return self.begin_close_scope(
                    pp.owner,
                    pp.previous,
                    ScopeOutcome::Discard(Box::new(paused)),
                );
            }
            self.close_mirrored_owner(pp.owner)?;
            self.close_owner(pp.owner)?;
            self.current_owner = pp.previous;
            return Ok(());
        }
        if self.owner_needs_async_close(inner_owner) {
            let paused = self.discard.take().expect("active transfer");
            return self.begin_close_scope(
                inner_owner,
                prev_owner,
                ScopeOutcome::Discard(Box::new(paused)),
            );
        }
        self.close_mirrored_owner(inner_owner)?;
        self.close_owner(inner_owner)?;
        Ok(())
    }

    pub(super) fn discard_foreign_layer_build_finish(
        &mut self,
        target: LayerBuildTarget,
        roots: Vec<Value>,
    ) -> Result<(), Error> {
        self.heap.drop_values(roots)?;
        if let LayerBuildTarget::Provide(pid) = target
            && let Some(pp) = self.pending_provides.remove(&pid)
        {
            self.heap.drop_value(pp.body_thunk)?;
            self.restore_fiber_memo(pp.prev_memo)?;
            self.current_evidence = pp.saved_evidence;
            if self.owner_needs_async_close(pp.owner) {
                let paused = self.discard.take().expect("active transfer");
                return self.begin_close_scope(
                    pp.owner,
                    pp.previous,
                    ScopeOutcome::Discard(Box::new(paused)),
                );
            }
            self.close_mirrored_owner(pp.owner)?;
            self.close_owner(pp.owner)?;
            self.current_owner = pp.previous;
        }
        Ok(())
    }

    fn release_memo_observer(
        &mut self,
        memo_id: usize,
        layer_id: ObjectId,
        entry_owner: usize,
    ) -> Option<usize> {
        let memo = self.memo_maps.get_mut(&memo_id)?;
        let entry = memo.entries.get_mut(&layer_id)?;
        if entry.scope_owner != entry_owner {
            return None;
        }
        entry.observers = entry.observers.saturating_sub(1);
        if entry.observers == 0 {
            let scope_owner = entry.scope_owner;
            if let Some(removed) = memo.entries.remove(&layer_id) {
                match removed.state {
                    MemoEntryState::Building { .. } => {}
                    MemoEntryState::Ready(v) => {
                        let _ = self.heap.drop_value(v);
                    }
                    MemoEntryState::Failed(c) => {
                        let _ = self.heap.drop_cause(c);
                    }
                }
            }
            Some(scope_owner)
        } else {
            None
        }
    }

    pub(super) fn find_dest_service<'a>(
        heap: &'a Heap,
        dest: &'a Destination<'p>,
        providers: &'a BTreeMap<usize, Provider>,
        effect: &str,
        fuel: &mut u64,
    ) -> Result<Option<Value>, Error> {
        match dest {
            Destination::LayerRun { provider, .. } => {
                if let Some(p) = providers.get(provider)
                    && let Some(imp) = layers::lookup_layer_service(heap, &p.layer, effect, fuel)?
                {
                    return Ok(Some(imp.representation()));
                }
                Ok(None)
            }
            Destination::ProvideContext { context, .. } => {
                if let Some(imp) = layers::lookup_layer_service(heap, context, effect, fuel)? {
                    return Ok(Some(imp.representation()));
                }
                Ok(None)
            }
            Destination::WithServices { services, next } => {
                for s in services {
                    if let Some(imp) = layers::lookup_layer_service(heap, s, effect, fuel)? {
                        return Ok(Some(imp.representation()));
                    }
                }
                Self::find_dest_service(heap, next, providers, effect, fuel)
            }
            Destination::RestoreMask { next, .. } | Destination::CachedComplete { next, .. } => {
                Self::find_dest_service(heap, next, providers, effect, fuel)
            }
            _ => Ok(None),
        }
    }

    fn lookup_service_impl(&mut self, effect: &str) -> Result<Option<Value>, Error> {
        let mut found_rep = None;
        for frame in self.frames.iter().rev() {
            if let Some(rep) = Self::find_dest_service(
                self.heap,
                &frame.destination,
                &self.providers,
                effect,
                &mut self.fuel,
            )? {
                found_rep = Some(rep);
                break;
            }
        }
        if found_rep.is_none() {
            for s in &self.inherited_services {
                if let Some(imp) =
                    layers::lookup_layer_service(self.heap, s, effect, &mut self.fuel)?
                {
                    found_rep = Some(imp.representation());
                    break;
                }
            }
        }
        if found_rep.is_none()
            && let Some(root) = self.root
            && let Some(imp) = layers::lookup_layer_service(
                self.heap,
                &self.providers[&root].layer,
                effect,
                &mut self.fuel,
            )?
        {
            found_rep = Some(imp.representation());
        }
        found_rep.map(|r| self.heap.dup(&r)).transpose()
    }

    fn cleanup_memo_maps(&mut self, roots: &mut Vec<Value>) {
        for m in mem::take(&mut self.memo_maps).into_values() {
            for entry in m.entries.into_values() {
                match entry.state {
                    MemoEntryState::Building { .. } => {}
                    MemoEntryState::Ready(v) => roots.push(v),
                    MemoEntryState::Failed(c) => roots.push(c.into_value()),
                }
            }
        }
    }
}
