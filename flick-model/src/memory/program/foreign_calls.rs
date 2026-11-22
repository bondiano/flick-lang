// Foreign call response waiting, completion, cancellation, and discarded handle cleanup.

impl<'p> Machine<'p, '_> {
    fn wait_for_call_response(
        &mut self,
        call_id: u64,
        destination: Destination<'p>,
        mut catch_fn: Option<Value>,
    ) -> Result<(), Error> {
        self.foreign.active_sync_calls.insert(call_id);
        loop {
            if let Some(done_msg) = self.foreign.completed_sync_calls.remove(&call_id) {
                self.foreign.active_sync_calls.remove(&call_id);
                self.foreign.pending_sync_calls.remove(&call_id);
                return self.complete_foreign_call_msg(&done_msg, destination, catch_fn.take());
            }
            if self.foreign.pending_sync_calls.remove(&call_id) {
                self.foreign.active_sync_calls.remove(&call_id);
                return self.park_foreign_async(call_id, destination, catch_fn.take());
            }
            let msg = match self
                .bridge
                .as_deref_mut()
                .expect("bridge opened")
                .recv_timeout(Duration::from_secs(10))
            {
                Ok(Some(m)) => m,
                Ok(None) => {
                    self.foreign.active_sync_calls.remove(&call_id);
                    self.drop_call_state(destination, catch_fn)?;
                    return Err(Error::HostBridge(
                        "bridge timed out waiting for call response".into(),
                    ));
                }
                Err(err) => {
                    self.foreign.active_sync_calls.remove(&call_id);
                    self.drop_call_state(destination, catch_fn)?;
                    return Err(err);
                }
            };
            match msg.get("type").and_then(Json::as_str) {
                Some("pending") if msg.get("call_id").and_then(Json::as_u64) == Some(call_id) => {
                    self.foreign.active_sync_calls.remove(&call_id);
                    return self.park_foreign_async(call_id, destination, catch_fn.take());
                }
                Some("call_done")
                    if msg.get("call_id").and_then(Json::as_u64) == Some(call_id) =>
                {
                    self.foreign.active_sync_calls.remove(&call_id);
                    return self.complete_foreign_call_msg(&msg, destination, catch_fn.take());
                }
                _ => {
                    if let Err(err) = self.dispatch_bridge_event(&msg) {
                        self.foreign.active_sync_calls.remove(&call_id);
                        self.drop_call_state(destination, catch_fn)?;
                        return Err(err);
                    }
                }
            }
        }
    }

    fn park_foreign_async(
        &mut self,
        call_id: u64,
        destination: Destination<'p>,
        catch_fn: Option<Value>,
    ) -> Result<(), Error> {
        self.foreign.pending_calls.insert(
            call_id,
            PendingForeignCall {
                fiber: self.active_fiber,
                destination,
                catch_fn,
            },
        );
        self.park_current(ParkReason::ForeignAsync { call_id })
    }

    fn drop_call_state(
        &mut self,
        destination: Destination<'p>,
        catch_fn: Option<Value>,
    ) -> Result<(), Error> {
        let mut roots = Vec::new();
        self.drain_destination_roots(destination, &mut roots);
        roots.extend(catch_fn);
        self.heap.drop_values(roots)
    }

    fn complete_foreign_call_msg(
        &mut self,
        msg: &Json,
        destination: Destination<'p>,
        catch_fn: Option<Value>,
    ) -> Result<(), Error> {
        if msg.get("sync_fd1").and_then(Json::as_bool) == Some(true)
            && let Some(bridge) = self.bridge.as_deref_mut()
        {
            bridge.drain_fd1_stdout(&mut self.stdout, true);
        }
        if msg.get("ok").and_then(Json::as_bool) == Some(true) {
            if let Some(cf) = catch_fn {
                self.heap.drop_value(cf)?;
            }
            let wire = msg
                .get("value")
                .ok_or_else(|| Error::HostBridge("call_done missing value".into()))?;
            let val = match wire::decode_value(self.heap, self.execution, wire) {
                Ok(v) => v,
                Err(err) => {
                    self.drop_call_state(destination, None)?;
                    return Err(err);
                }
            };
            self.pending = Some(val);
            return self.returned_frame(destination);
        }
        self.complete_foreign_call_error(msg, destination, catch_fn)
    }

    fn complete_foreign_call_error(
        &mut self,
        msg: &Json,
        destination: Destination<'p>,
        catch_fn: Option<Value>,
    ) -> Result<(), Error> {
        let kind = msg.get("kind").and_then(Json::as_str).unwrap_or("die");
        match kind {
            "catch" => {
                let cf = catch_fn.ok_or_else(|| {
                    Error::HostBridge("host returned catch error without catch_fn".into())
                })?;
                let err_wire = msg
                    .get("error")
                    .ok_or_else(|| Error::HostBridge("catch error missing payload".into()))?;
                let err_val = match wire::decode_value(self.heap, self.execution, err_wire) {
                    Ok(v) => v,
                    Err(err) => {
                        self.heap.drop_value(cf)?;
                        self.drop_call_state(destination, None)?;
                        return Err(err);
                    }
                };
                self.drop_call_state(destination, None)?;
                self.scratch.extend([cf, err_val]);
                self.apply_fresh(Destination::RaiseFromCatch)
            }
            "cause" => {
                if let Some(cf) = catch_fn {
                    self.heap.drop_value(cf)?;
                }
                let cause_json = msg
                    .get("cause")
                    .ok_or_else(|| Error::HostBridge("cause error missing payload".into()))?;
                let cause = match wire::decode_cause_wire(self.heap, self.execution, cause_json) {
                    Ok(c) => c,
                    Err(err) => {
                        self.drop_call_state(destination, None)?;
                        return Err(err);
                    }
                };
                self.failure = Some(cause);
                self.failure_elected = true;
                self.fail_destination(destination)
            }
            "missing_handler" => {
                if let Some(cf) = catch_fn {
                    self.heap.drop_value(cf)?;
                }
                self.drop_call_state(destination, None)?;
                let eff = msg.get("effect").and_then(Json::as_str).unwrap_or("?");
                let op = msg.get("op_name").and_then(Json::as_str).unwrap_or("?");
                Err(Error::MissingHandler {
                    effect: eff.to_owned(),
                    op: op.to_owned(),
                })
            }
            _ => {
                if let Some(cf) = catch_fn {
                    self.heap.drop_value(cf)?;
                }
                let text = msg
                    .get("message")
                    .and_then(Json::as_str)
                    .unwrap_or("host defect");
                self.drop_call_state(destination, None)?;
                self.raise_defect(DefectSpec::custom(text))
            }
        }
    }

    fn collect_wire_handles(node_val: &Json, out: &mut Vec<u64>) {
        if let Some(nodes) = node_val.get("nodes").and_then(Json::as_array) {
            for node in nodes {
                if let Some(arr) = node.as_array()
                    && arr.first().and_then(Json::as_str) == Some("handle")
                    && let Some(id) = arr.get(1).and_then(Json::as_u64)
                {
                    out.push(id);
                }
            }
        }
    }

    fn release_discarded_call_handles(&mut self, msg: &Json) -> Result<(), Error> {
        let mut ids = Vec::new();
        if let Some(v) = msg.get("value") {
            Self::collect_wire_handles(v, &mut ids);
        }
        if let Some(e) = msg.get("error") {
            Self::collect_wire_handles(e, &mut ids);
        }
        if let Some(reasons) = msg
            .get("cause")
            .and_then(|c| c.get("reasons"))
            .and_then(Json::as_array)
        {
            for r in reasons {
                if let Some(arr) = r.as_array()
                    && arr.first().and_then(Json::as_str) == Some("fail")
                    && let Some(v) = arr.get(1)
                {
                    Self::collect_wire_handles(v, &mut ids);
                }
            }
        }
        if !ids.is_empty()
            && self.foreign.bridge_opened
            && let Some(bridge) = self.bridge.as_deref_mut()
        {
            bridge.release_handles(self.execution, &ids)?;
        }
        Ok(())
    }

    fn resolve_pending_async_call(&mut self, call_id: u64, msg: &Json) -> Result<(), Error> {
        let Some(pending) = self.foreign.pending_calls.remove(&call_id) else {
            return self.release_discarded_call_handles(msg);
        };
        let target = pending.fiber;
        let is_still_parked = self.fiber_table.get(&target).is_some_and(|r| {
            matches!(r.parked, Some(ParkReason::ForeignAsync { call_id: c }) if c == call_id)
        });
        if !is_still_parked {
            self.release_discarded_call_handles(msg)?;
            return self.drop_call_state(pending.destination, pending.catch_fn);
        }
        if let Some(rec) = self.fiber_table.get_mut(&target) {
            rec.parked = None;
        }
        let caller = self.active_fiber;
        self.switch_to_fiber(target);
        let res = self.complete_foreign_call_msg(msg, pending.destination, pending.catch_fn);
        self.switch_to_fiber(caller);
        res?;
        self.run_queue.push_back(target);
        Ok(())
    }

    fn cancel_foreign_call(&mut self, call_id: u64) -> Result<(), Error> {
        if let Some(pending) = self.foreign.pending_calls.remove(&call_id) {
            self.drop_call_state(pending.destination, pending.catch_fn)?;
        }
        if self.foreign.bridge_opened
            && let Some(bridge) = self.bridge.as_deref_mut()
        {
            bridge.send(&json!({
                "op": "cancel",
                "exec": self.execution,
                "call_id": call_id,
            }))?;
        }
        Ok(())
    }
}
