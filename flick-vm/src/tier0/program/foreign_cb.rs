// Host-to-Flick callback invocation (sync dict getters, sync-or-async child fibers, async fibers).

type CbFiberInputs = (Value, Vec<Value>, BTreeMap<String, Value>);

impl Machine<'_, '_> {
    fn handle_cb_invoke(&mut self, msg: &Json) -> Result<(), Error> {
        let mode = msg.get("mode").and_then(Json::as_str).unwrap_or("sync");
        match mode {
            "sync" => {
                let is_dict_getter = msg
                    .get("cb_id")
                    .and_then(Json::as_u64)
                    .is_some_and(|id| self.foreign.dict_getters.contains_key(&id));
                if is_dict_getter {
                    self.handle_sync_cb_invoke(msg)
                } else {
                    self.handle_sync_child_invoke(msg, false)
                }
            }
            "sync_task" => self.handle_sync_child_invoke(msg, true),
            "sync_effect" => self.handle_sync_child_invoke(msg, false),
            _ => self.spawn_async_cb_fiber(msg),
        }
    }

    fn decode_cb_args(&mut self, msg: &Json) -> Result<VecDeque<Value>, Error> {
        let arr = msg
            .get("args")
            .and_then(Json::as_array)
            .ok_or_else(|| Error::HostBridge("cb_invoke missing args".into()))?;
        let mut out = VecDeque::with_capacity(arr.len());
        for wire in arr {
            match wire::decode_value(self.heap, self.execution, wire) {
                Ok(v) => out.push_back(v),
                Err(err) => {
                    let _ = self.heap.drop_values(out.into_iter().collect());
                    return Err(err);
                }
            }
        }
        Ok(out)
    }

    fn send_sync_cb_err(&mut self, req_id: u64, message: &str) {
        if let Some(bridge) = self.bridge.as_deref_mut() {
            let _ = bridge.send(&json!({
                "op": "cb_return",
                "exec": self.execution,
                "req_id": req_id,
                "ok": false,
                "message": message,
            }));
        }
    }

    fn prepare_cb_fiber_inputs(
        &mut self,
        msg: &Json,
        req_id: u64,
        cb_id: u64,
    ) -> Result<CbFiberInputs, Error> {
        let Some(cb_rep) = self.foreign.callbacks.get(&cb_id).map(Value::representation) else {
            if let Some(hid) = msg.get("host_ctx_handle").and_then(Json::as_u64)
                && let Some(bridge) = self.bridge.as_deref_mut()
            {
                let _ = bridge.release_handles(self.execution, &[hid]);
            }
            self.send_sync_cb_err(req_id, "unknown callback id");
            return Err(Error::HostBridge(format!("unknown callback id {cb_id}")));
        };
        let mut services = self.snapshot_active_services()?;
        if let Some(hid) = msg.get("host_ctx_handle").and_then(Json::as_u64) {
            let h = self.heap.allocate(Data::ForeignHandle {
                execution: self.execution,
                host_id: hid,
            });
            match self.wrap_host_context_handle(h) {
                Ok(wrapped) => services.insert(0, wrapped),
                Err(err) => {
                    let _ = self.heap.drop_values(services);
                    self.send_sync_cb_err(req_id, &err.to_string());
                    return Err(err);
                }
            }
        }
        let refs = self.snapshot_fiber_refs()?;
        let callee = match self.heap.dup(&cb_rep) {
            Ok(v) => v,
            Err(err) => {
                let _ = self.heap.drop_values(services);
                let _ = self.heap.drop_values(refs.into_values().collect());
                self.send_sync_cb_err(req_id, &err.to_string());
                return Err(err);
            }
        };
        Ok((callee, services, refs))
    }

    fn cb_owner_context(&self, msg: &Json) -> (usize, Option<OwnerEvidence>) {
        let owner = msg
            .get("owner")
            .and_then(Json::as_u64)
            .and_then(|o| usize::try_from(o).ok())
            .filter(|&o| o < self.owners.len())
            .unwrap_or(self.current_owner);
        let evidence = msg
            .get("evidence_owner")
            .and_then(Json::as_u64)
            .and_then(|o| usize::try_from(o).ok())
            .filter(|&o| o < self.owners.len())
            .map(|o| OwnerEvidence::from_owner(&self.owner_handle(o)))
            .or(self.current_evidence);
        (owner, evidence)
    }

    fn spawn_cb_child(
        &mut self,
        msg: &Json,
        req_id: u64,
        is_task: bool,
        callee: Value,
        services: Vec<Value>,
        refs: BTreeMap<String, Value>,
    ) -> Result<usize, Error> {
        let (owner, evidence) = self.cb_owner_context(msg);
        if is_task {
            return self.spawn_raw_fiber(owner, evidence, callee, services, refs, 0);
        }
        let mut args = match self.decode_cb_args(msg) {
            Ok(a) => a,
            Err(err) => {
                let _ = self.heap.drop_value(callee);
                let _ = self.heap.drop_values(services);
                let _ = self.heap.drop_values(refs.into_values().collect());
                self.send_sync_cb_err(req_id, &err.to_string());
                return Err(err);
            }
        };
        let first_arg = args.pop_front().unwrap_or(Value::Unit);
        self.spawn_host_cb_fiber(owner, evidence, callee, first_arg, args, services, refs)
    }

    fn handle_sync_child_invoke(&mut self, msg: &Json, is_task: bool) -> Result<(), Error> {
        let req_id = msg
            .get("req_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("sync_child missing req_id".into()))?;
        let cb_id = msg
            .get("cb_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("sync_child missing cb_id".into()))?;
        let (callee, services, fiber_refs) = self.prepare_cb_fiber_inputs(msg, req_id, cb_id)?;
        let child_id = self.spawn_cb_child(msg, req_id, is_task, callee, services, fiber_refs)?;
        if self.run_queue.back() == Some(&child_id) {
            self.run_queue.pop_back();
        }
        let parent_fiber = self.active_fiber;
        let prev_sync = self.foreign.sync_task_fiber.replace(child_id);
        self.switch_to_fiber(child_id);
        let outcome = self.run_until_sync_task(child_id);
        self.foreign.sync_task_fiber = prev_sync;
        self.switch_to_fiber(parent_fiber);
        match outcome {
            Ok(Some(sync_out)) => {
                self.drop_finished_sync_task_fiber(child_id)?;
                self.send_cb_return_outcome(req_id, sync_out)
            }
            Ok(None) => {
                self.foreign.pending_host_cbs.insert(child_id, req_id);
                self.bridge.as_deref_mut().expect("bridge opened").send(&json!({
                    "op": "cb_return",
                    "exec": self.execution,
                    "req_id": req_id,
                    "ok": false,
                    "kind": "async_needed",
                }))
            }
            Err(err) => {
                let _ = self.drop_finished_sync_task_fiber(child_id);
                self.send_sync_cb_err(req_id, &err.to_string());
                Err(err)
            }
        }
    }

    fn run_until_sync_task(
        &mut self,
        child_id: usize,
    ) -> Result<Option<Result<Value, CauseToken>>, Error> {
        loop {
            if let Some(owner) = self.pending_resume_close.take() {
                self.resume_owner_close(owner)?;
                continue;
            }
            if let Some(res) = self.pending_provide_result.take() {
                match res {
                    PendingProvideResult::Ready(pid, ctx) => {
                        self.complete_dynamic_provide_ok(pid, ctx)?;
                    }
                    PendingProvideResult::Failed(pid, cause) => {
                        self.complete_dynamic_provide_err(pid, cause)?;
                    }
                }
                continue;
            }
            if let Some((layer, memo_id, sub_owner)) = self.pending_layer_build.take() {
                self.begin_build_layer(layer, memo_id, sub_owner, false, LayerBuildTarget::Top)?;
                continue;
            }
            if self
                .fiber_table
                .get(&child_id)
                .is_some_and(|r| r.parked.is_some())
            {
                return Ok(None);
            }
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            self.drain_dropped_continuations();
            if self.frames.is_empty() && self.discard.is_none() && self.ready.is_none() {
                if let Some(cause) = self.failure.take() {
                    self.failure_elected = false;
                    return Ok(Some(Err(cause)));
                }
                if self.arguments.is_empty() {
                    return Ok(Some(Ok(self.top.take().unwrap_or(Value::Unit))));
                }
            }
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            if self.discard.is_some() {
                self.discard_step()?;
            } else if let Some(id) = self.ready.take() {
                self.invoke_step(id)?;
            } else if self.failure.is_some() {
                self.unwind()?;
            } else if let Some(value) = self.top.take() {
                self.scratch.push(value);
                self.scratch.push(
                    self.arguments
                        .pop_front()
                        .expect("sync task top application has an argument"),
                );
                self.apply(Destination::Top)?;
            } else {
                self.step()?;
            }
        }
    }

    fn drop_finished_sync_task_fiber(&mut self, child_id: usize) -> Result<(), Error> {
        self.fiber_table.remove(&child_id);
        if let Some(ctx) = self.parked_fibers.remove(&child_id) {
            let mut roots = Vec::new();
            self.drain_fiber_ctx_roots(ctx, &mut roots);
            self.heap.drop_values(roots)?;
        }
        Ok(())
    }

    fn send_cb_return_outcome(
        &mut self,
        req_id: u64,
        outcome: Result<Value, CauseToken>,
    ) -> Result<(), Error> {
        self.flush_released_handles()?;
        let (ok, field_key, payload_val) = match outcome {
            Ok(val) => (true, "value", val),
            Err(cause) => (false, "cause", cause.into_value()),
        };
        let wire = match wire::encode_value(
            self.heap,
            self.execution,
            &payload_val,
            None,
            None,
            &mut self.foreign,
        ) {
            Ok(w) => w,
            Err(err) => {
                let _ = self.heap.drop_value(payload_val);
                self.send_sync_cb_err(req_id, &err.to_string());
                return Err(err);
            }
        };
        let msg = json!({
            "op": "cb_return",
            "exec": self.execution,
            "req_id": req_id,
            "ok": ok,
            field_key: wire,
        });
        let send_res = self.bridge.as_deref_mut().expect("bridge opened").send(&msg);
        let drop_res = self.heap.drop_value(payload_val);
        send_res?;
        drop_res?;
        self.flush_released_handles()
    }

    fn handle_sync_cb_invoke(&mut self, msg: &Json) -> Result<(), Error> {
        if let Some(hid) = msg.get("host_ctx_handle").and_then(Json::as_u64)
            && let Some(bridge) = self.bridge.as_deref_mut()
        {
            bridge.release_handles(self.execution, &[hid])?;
        }
        let req_id = msg
            .get("req_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("cb_invoke missing req_id".into()))?;
        let cb_id = msg
            .get("cb_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("cb_invoke missing cb_id".into()))?;
        let (callee, first_arg, args) = self.resolve_sync_cb_target(msg, req_id, cb_id)?;
        let saved_result = self.foreign.sync_cb_result.take();
        self.scratch.extend([callee, first_arg]);
        let start_res = self.apply_fresh(Destination::HostSyncCallback {
            req_id,
            remaining_args: args,
        });
        if let Err(err) = start_res {
            self.foreign.sync_cb_result = saved_result;
            self.send_sync_cb_err(req_id, &err.to_string());
            return Err(err);
        }
        let outcome = match self.run_until_sync_cb(req_id) {
            Ok(out) => out,
            Err(err) => {
                self.foreign.sync_cb_result = saved_result;
                self.send_sync_cb_err(req_id, &err.to_string());
                return Err(err);
            }
        };
        self.foreign.sync_cb_result = saved_result;
        self.send_cb_return_outcome(req_id, outcome)
    }

    fn resolve_sync_cb_target(
        &mut self,
        msg: &Json,
        req_id: u64,
        cb_id: u64,
    ) -> Result<(Value, Value, VecDeque<Value>), Error> {
        if let Some((dict_v, getter_v)) = self.foreign.dict_getters.get(&cb_id) {
            let drep = dict_v.representation();
            let grep = getter_v.representation();
            let getter = match self.heap.dup(&grep) {
                Ok(c) => c,
                Err(err) => {
                    self.send_sync_cb_err(req_id, &err.to_string());
                    return Err(err);
                }
            };
            let dict = match self.heap.dup(&drep) {
                Ok(d) => d,
                Err(err) => {
                    let _ = self.heap.drop_value(getter);
                    self.send_sync_cb_err(req_id, &err.to_string());
                    return Err(err);
                }
            };
            return Ok((getter, dict, VecDeque::new()));
        }
        let Some(cb_rep) = self.foreign.callbacks.get(&cb_id).map(Value::representation) else {
            self.send_sync_cb_err(req_id, "unknown callback id");
            return Err(Error::HostBridge(format!("unknown callback id {cb_id}")));
        };
        let mut args = match self.decode_cb_args(msg) {
            Ok(a) => a,
            Err(err) => {
                self.send_sync_cb_err(req_id, &err.to_string());
                return Err(err);
            }
        };
        let first_arg = args.pop_front().unwrap_or(Value::Unit);
        let callee = match self.heap.dup(&cb_rep) {
            Ok(c) => c,
            Err(err) => {
                let _ = self.heap.drop_value(first_arg);
                let _ = self.heap.drop_values(args.into_iter().collect());
                self.send_sync_cb_err(req_id, &err.to_string());
                return Err(err);
            }
        };
        Ok((callee, first_arg, args))
    }

    fn run_until_sync_cb(&mut self, req_id: u64) -> Result<Result<Value, CauseToken>, Error> {
        loop {
            if let Some((done_id, _)) = self.foreign.sync_cb_result.as_ref()
                && *done_id == req_id
            {
                let (_, res) = self.foreign.sync_cb_result.take().expect("sync cb result");
                return Ok(res);
            }
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            self.drain_dropped_continuations();
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            if self.discard.is_some() {
                self.discard_step()?;
            } else if let Some(id) = self.ready.take() {
                self.invoke_step(id)?;
            } else if self.failure.is_some() {
                self.unwind()?;
            } else if !self.frames.is_empty() {
                self.step()?;
            } else {
                return Err(Error::HostBridge(
                    "sync callback returned without reaching HostSyncCallback boundary".into(),
                ));
            }
        }
    }

    fn spawn_async_cb_fiber(&mut self, msg: &Json) -> Result<(), Error> {
        let req_id = msg
            .get("req_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("async cb_invoke missing req_id".into()))?;
        let cb_id = msg
            .get("cb_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("async cb_invoke missing cb_id".into()))?;
        let is_task = msg.get("mode").and_then(Json::as_str) == Some("task");
        let (callee, services, refs) = self.prepare_cb_fiber_inputs(msg, req_id, cb_id)?;
        let child_id = self.spawn_cb_child(msg, req_id, is_task, callee, services, refs)?;
        self.foreign.pending_host_cbs.insert(child_id, req_id);
        Ok(())
    }

    #[expect(clippy::too_many_arguments, reason = "host callback fiber spawn context")]
    fn spawn_host_cb_fiber(
        &mut self,
        owner: usize,
        evidence: Option<OwnerEvidence>,
        func: Value,
        first_arg: Value,
        remaining_args: VecDeque<Value>,
        services: Vec<Value>,
        fiber_refs: BTreeMap<String, Value>,
    ) -> Result<usize, Error> {
        let tail_ops = match self.snapshot_active_tail_ops() {
            Ok(ops) => ops,
            Err(err) => {
                let _ = self.heap.drop_values(vec![func, first_arg]);
                let _ = self.heap.drop_values(remaining_args.into_iter().collect());
                let _ = self.heap.drop_values(services);
                let _ = self.heap.drop_values(fiber_refs.into_values().collect());
                return Err(err);
            }
        };
        let child_id = self.alloc_fiber_id();
        let mut ctx = FiberContext::empty(owner, evidence);
        ctx.inherited_services = services;
        ctx.inherited_tail_ops = tail_ops;
        ctx.fiber_refs = fiber_refs;
        ctx.seeded_random = self.seeded_random;
        let caller = self.active_fiber;
        self.parked_fibers.insert(child_id, ctx);
        self.fiber_table.insert(child_id, FiberRecord::root());
        self.switch_to_fiber(child_id);
        self.scratch.extend([func, first_arg]);
        let dest = if remaining_args.is_empty() {
            Destination::Top
        } else {
            Destination::HostApplyChain { remaining_args }
        };
        let res = self.apply_fresh(dest);
        self.switch_to_fiber(caller);
        if let Err(err) = res {
            self.drop_failed_spawn(child_id)?;
            return Err(err);
        }
        self.run_queue.push_back(child_id);
        Ok(child_id)
    }

    fn send_async_cb_return(&mut self, req_id: u64, fid: usize) -> Result<(), Error> {
        let Some(rec) = self.fiber_table.remove(&fid) else {
            return Ok(());
        };
        match rec.outcome {
            FiberOutcome::Success(val) => self.send_cb_return_outcome(req_id, Ok(val)),
            FiberOutcome::Failure(cause) => self.send_cb_return_outcome(req_id, Err(cause)),
            FiberOutcome::Running => Ok(()),
        }
    }
}
