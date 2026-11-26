// Bridge event polling, child fiber pumping during owner close, equality, and teardown.

impl Machine<'_, '_> {
    fn has_pending_foreign_work(&self) -> bool {
        self.foreign.bridge_opened
            && (!self.foreign.pending_calls.is_empty()
                || !self.foreign.pending_host_cbs.is_empty()
                || !self.foreign.callbacks.is_empty())
    }

    fn try_poll_bridge_nonblocking(&mut self) -> Result<bool, Error> {
        if !self.has_pending_foreign_work() {
            return Ok(false);
        }
        self.flush_released_handles()?;
        let mut progressed = false;
        while let Some(msg) = self
            .bridge
            .as_deref_mut()
            .expect("bridge opened")
            .try_recv()?
        {
            self.dispatch_bridge_event(&msg)?;
            progressed = true;
            if !self.run_queue.is_empty() {
                break;
            }
        }
        Ok(progressed)
    }

    fn poll_bridge_once(&mut self) -> Result<(), Error> {
        self.flush_released_handles()?;
        loop {
            let Some(msg) = self
                .bridge
                .as_deref_mut()
                .expect("bridge opened")
                .recv_timeout(Duration::from_secs(10))?
            else {
                return Err(Error::HostBridge(
                    "bridge timed out waiting for async event".into(),
                ));
            };
            self.dispatch_bridge_event(&msg)?;
            if !self.run_queue.is_empty()
                || !self.timers.is_empty()
                || !self.has_pending_foreign_work()
            {
                return Ok(());
            }
        }
    }

    fn dispatch_bridge_event(&mut self, msg: &Json) -> Result<(), Error> {
        match msg.get("type").and_then(Json::as_str) {
            Some("stdout") => {
                if let Some(text) = msg.get("text").and_then(Json::as_str) {
                    self.stdout.push_str(text);
                }
            }
            Some("call_done") => {
                if let Some(cid) = msg.get("call_id").and_then(Json::as_u64) {
                    if self.foreign.active_sync_calls.contains(&cid) {
                        self.foreign.completed_sync_calls.insert(cid, msg.clone());
                    } else {
                        self.resolve_pending_async_call(cid, msg)?;
                    }
                }
            }
            Some("pending") => {
                if let Some(cid) = msg.get("call_id").and_then(Json::as_u64)
                    && self.foreign.active_sync_calls.contains(&cid)
                {
                    self.foreign.pending_sync_calls.insert(cid);
                }
            }
            Some("cb_invoke") => {
                self.handle_cb_invoke(msg)?;
            }
            Some("cb_cancel") => {
                self.handle_cb_cancel(msg)?;
            }
            Some("cell_get") => {
                self.handle_cell_get(msg)?;
            }
            Some("cell_set") => {
                self.handle_cell_set(msg)?;
            }
            Some("owner_closed") => {
                if let Some(o) = msg.get("owner").and_then(Json::as_u64)
                    && let Ok(idx) = usize::try_from(o)
                {
                    self.foreign.closed_mirrored_owners.insert(idx);
                }
            }
            Some("fatal") => {
                let text = msg
                    .get("message")
                    .and_then(Json::as_str)
                    .unwrap_or("worker fatal");
                return Err(Error::HostBridge(text.to_owned()));
            }
            _ => {}
        }
        Ok(())
    }

    fn pump_child_fibers_while_blocked(&mut self, blocked_fiber: usize) -> Result<(), Error> {
        loop {
            let next_pos = self.run_queue.iter().position(|&id| id != blocked_fiber);
            let Some(pos) = next_pos else {
                return Ok(());
            };
            let child_id = self.run_queue.remove(pos).expect("queued child fiber");
            if !self.parked_fibers.contains_key(&child_id) {
                continue;
            }
            self.switch_to_fiber(child_id);
            let res = self.drive_single_child_until_yield_or_done(blocked_fiber);
            if self.active_fiber != blocked_fiber && self.parked_fibers.contains_key(&blocked_fiber)
            {
                self.switch_to_fiber(blocked_fiber);
            }
            res?;
        }
    }

    fn finish_child_to_blocked(
        &mut self,
        blocked_fiber: usize,
        outcome: FiberOutcome,
    ) -> Result<(), Error> {
        let fid = self.active_fiber;
        self.record_child_fiber_completion(fid, outcome)?;
        let mut next_ctx = self
            .parked_fibers
            .remove(&blocked_fiber)
            .expect("blocked fiber context");
        self.swap_fiber_context(&mut next_ctx);
        let mut roots = Vec::new();
        self.drain_fiber_ctx_roots(next_ctx, &mut roots);
        self.heap.drop_values(roots)?;
        self.active_fiber = blocked_fiber;
        Ok(())
    }

    fn drive_single_child_until_yield_or_done(&mut self, blocked_fiber: usize) -> Result<(), Error> {
        loop {
            if self.active_fiber == blocked_fiber {
                return Ok(());
            }
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
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            self.drain_dropped_continuations();
            if self.discard.is_none()
                && self.ready.is_none()
                && self.failure.is_some()
                && self.frames.is_empty()
            {
                let cause = self.failure.take().expect("child failure");
                return self.finish_child_to_blocked(blocked_fiber, FiberOutcome::Failure(cause));
            }
            if self.discard.is_none()
                && self.ready.is_none()
                && self.frames.is_empty()
                && self.arguments.is_empty()
                && self.top.is_some()
            {
                let val = self.top.take().expect("child return");
                return self.finish_child_to_blocked(blocked_fiber, FiberOutcome::Success(val));
            }
            if self
                .fiber_table
                .get(&self.active_fiber)
                .is_some_and(|r| r.parked.is_some())
            {
                return Ok(());
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
                        .expect("child fiber top application has an argument"),
                );
                self.apply(Destination::Top)?;
            } else if !self.frames.is_empty() {
                self.step()?;
            } else {
                return Ok(());
            }
        }
    }

    fn try_foreign_handles_equal(&mut self) -> Result<Option<bool>, Error> {
        let (Value::Object(a), Value::Object(b)) = (&self.scratch[0], &self.scratch[1]) else {
            return Ok(None);
        };
        let (
            Data::ForeignHandle {
                execution: ae,
                host_id: ah,
            },
            Data::ForeignHandle {
                execution: be,
                host_id: bh,
            },
        ) = (&self.heap.object(a.id)?.data, &self.heap.object(b.id)?.data)
        else {
            return Ok(None);
        };
        if *ae != self.execution || *be != self.execution {
            return Ok(Some(false));
        }
        if *ah == *bh {
            return Ok(Some(true));
        }
        let (ah, bh) = (*ah, *bh);
        self.flush_released_handles()?;
        let eq = self
            .bridge
            .as_deref_mut()
            .expect("bridge opened")
            .equals_handles(self.execution, ah, bh, &mut self.stdout)?;
        Ok(Some(eq))
    }

    fn handle_cb_cancel(&mut self, msg: &Json) -> Result<(), Error> {
        let Some(req_id) = msg.get("req_id").and_then(Json::as_u64) else {
            return Ok(());
        };
        let child = self
            .foreign
            .pending_host_cbs
            .iter()
            .find(|(_, id)| **id == req_id)
            .map(|(&fid, _)| fid);
        if let Some(child_id) = child {
            self.request_interrupt(child_id, 0)?;
        }
        Ok(())
    }

    fn close_mirrored_owner(&mut self, owner: usize) -> Result<(), Error> {
        if !self.foreign.bridge_opened || !self.foreign.mirrored_owners.remove(&owner) {
            return Ok(());
        }
        if self.foreign.closed_mirrored_owners.remove(&owner) {
            return Ok(());
        }
        let exec = self.execution;
        let blocked = self.active_fiber;
        let Some(bridge) = self.bridge.as_deref_mut() else {
            return Ok(());
        };
        bridge.send(&json!({ "op": "close_owner", "exec": exec, "owner": owner }))?;
        let deadline_wall = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump_child_fibers_while_blocked(blocked)?;
            if self.foreign.closed_mirrored_owners.remove(&owner) {
                return Ok(());
            }
            let now_wall = Instant::now();
            if now_wall >= deadline_wall {
                return Err(Error::HostBridge(
                    "bridge timed out waiting for owner close".into(),
                ));
            }
            let wait_budget = if let Some((&(timer_dl, _), _)) = self.timers.first_key_value() {
                let rem_ns = timer_dl.saturating_sub(self.virtual_now_ns);
                if rem_ns == 0 {
                    self.fire_due_timers()?;
                    continue;
                }
                Duration::from_nanos(rem_ns.min(25_000_000))
            } else {
                deadline_wall.saturating_duration_since(now_wall)
            };
            let started = Instant::now();
            let Some(msg) = self
                .bridge
                .as_deref_mut()
                .expect("bridge opened")
                .recv_timeout(wait_budget)?
            else {
                if !self.timers.is_empty() {
                    let elapsed_ns = u64::try_from(started.elapsed().as_nanos())
                        .unwrap_or(25_000_000)
                        .max(u64::try_from(wait_budget.as_nanos()).unwrap_or(25_000_000));
                    self.virtual_now_ns = self.virtual_now_ns.saturating_add(elapsed_ns);
                    self.fire_due_timers()?;
                    continue;
                }
                return Err(Error::HostBridge(
                    "bridge timed out waiting for owner close".into(),
                ));
            };
            if !self.timers.is_empty() {
                let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(0);
                self.virtual_now_ns = self.virtual_now_ns.saturating_add(elapsed_ns);
                self.fire_due_timers()?;
            }
            self.dispatch_bridge_event(&msg)?;
        }
    }

    fn cleanup_foreign_roots(&mut self, roots: &mut Vec<Value>) {
        roots.extend(mem::take(&mut self.foreign.callbacks).into_values());
        self.foreign.cb_by_obj.clear();
        for (d, g) in mem::take(&mut self.foreign.dict_getters).into_values() {
            roots.push(d);
            roots.push(g);
        }
        self.foreign.dict_getter_by_obj.clear();
        self.foreign.cells.clear();
        self.foreign.cell_by_id.clear();
        for pending in mem::take(&mut self.foreign.pending_calls).into_values() {
            self.drain_destination_roots(pending.destination, roots);
            roots.extend(pending.catch_fn);
        }
        self.foreign.pending_host_cbs.clear();
        if let Some((_, res)) = self.foreign.sync_cb_result.take() {
            match res {
                Ok(v) => roots.push(v),
                Err(c) => roots.push(c.into_value()),
            }
        }
    }

    fn finish_foreign_bridge(&mut self) -> Result<(), Error> {
        if !self.foreign.bridge_opened {
            let _ = self.heap.take_released_foreign_handles(self.execution);
            return Ok(());
        }
        self.flush_released_handles()?;
        self.foreign.bridge_opened = false;
        self.foreign.mirrored_owners.clear();
        self.foreign.closed_mirrored_owners.clear();
        self.foreign.active_sync_calls.clear();
        self.foreign.pending_sync_calls.clear();
        for msg in mem::take(&mut self.foreign.completed_sync_calls).into_values() {
            let _ = self.release_discarded_call_handles(&msg);
        }
        let exec = self.execution;
        if let Some(bridge) = self.bridge.as_deref_mut() {
            let live = bridge.close_exec(exec, &mut self.stdout)?;
            if live > 0 {
                return Err(Error::LeakedForeignHandles(live));
            }
        }
        Ok(())
    }
}
