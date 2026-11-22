// Bidirectional host Layer/Context, active service propagation, constructor tag
// registration, cell proxies, and real-time timer bridge polling.

use std::time::{Duration as StdDuration, Instant as StdInstant};

const HOST_CTX_KEY: &str = "flick/bridge/HostContext";

impl Machine<'_, '_> {
    /// Collects every constructor tag and built-in `@flick-lang/*` module specifier
    /// referenced anywhere in `self.program` so the Node worker can register FNV
    /// tag hashes and preload built-in modules once at `open_exec`.
    fn collect_program_metadata(&self) -> (Vec<String>, Vec<String>) {
        let mut tags = BTreeSet::new();
        let mut imports = BTreeSet::new();
        let mut blocks: Vec<&Block> = self.program.functions.values().map(|f| &f.body).collect();
        while let Some(block) = blocks.pop() {
            for ins in &block.instructions {
                if let Instruction::Let { value, .. } = ins {
                    match value {
                        Operation::Construct { tag, .. }
                        | Operation::ConstructWithDicts { tag, .. }
                        | Operation::Reuse { tag, .. }
                        | Operation::ReuseWithDicts { tag, .. }
                            if !tag.starts_with('$') =>
                        {
                            tags.insert(tag.clone());
                        }
                        Operation::Match { pattern, .. } => {
                            collect_pat_tags(pattern, &mut tags);
                        }
                        Operation::ForeignCall { spec, .. } => {
                            if let Some(from) = &spec.from
                                && (from.starts_with("@flick-lang/") || from.starts_with("flick:"))
                            {
                                imports.insert(from.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Exit::Branch { then_, else_, .. } = &block.exit {
                blocks.push(then_);
                blocks.push(else_);
            }
        }
        (tags.into_iter().collect(), imports.into_iter().collect())
    }

    fn wrap_host_context_handle(&mut self, handle: Value) -> Result<Value, Error> {
        let list = self.heap.construct("$list", vec![handle])?;
        let mut map = BTreeMap::new();
        map.insert(HOST_CTX_KEY.to_owned(), list);
        Ok(self.heap.allocate(Data::Context(map)))
    }

    fn concat_host_context_lists(&mut self, first: Value, second: Value) -> Result<Value, Error> {
        let mut items = Vec::new();
        for list in [first, second] {
            let count = match self.heap.list_view(&list) {
                Ok((_, _, c)) => c,
                Err(err) => {
                    let _ = self.heap.drop_value(list);
                    let _ = self.heap.drop_values(items);
                    return Err(err);
                }
            };
            for i in 0..count {
                match self.heap.project(&list, i) {
                    Ok(v) => items.push(v),
                    Err(err) => {
                        let _ = self.heap.drop_value(list);
                        let _ = self.heap.drop_values(items);
                        return Err(err);
                    }
                }
            }
            self.heap.drop_value(list)?;
        }
        self.heap.construct("$list", items)
    }

    /// Encodes all active non-root service layers/contexts outermost-to-innermost so
    /// JS `buildCallCtx` reflects active Rust `provide` and host `Layer` scopes.
    fn collect_active_services_wire(&mut self) -> Result<Vec<Json>, Error> {
        let mut reps = Vec::new();
        for s in self.inherited_services.iter().rev() {
            reps.push(s.representation());
        }
        for frame in &self.frames {
            Self::collect_dest_services(&frame.destination, &self.providers, &mut reps);
        }
        let mut flat: Vec<(String, Value)> = Vec::new();
        for rep in reps {
            self.extract_layer_entries(&rep, &mut flat)?;
        }
        let mut out = Vec::new();
        for (key, val_rep) in flat {
            if key == HOST_CTX_KEY {
                if let Ok((_, _, count)) = self.heap.list_view(&val_rep) {
                    for i in 0..count {
                        let item = self.heap.field(&val_rep, i)?;
                        if let Value::Object(tok) = item
                            && let Data::ForeignHandle { host_id, .. } =
                                self.heap.object(tok.id)?.data
                        {
                            out.push(json!(["host_ctx", host_id]));
                        }
                    }
                }
                continue;
            }
            if key == MEMO_REF_KEY {
                continue;
            }
            if let Ok(wire) = wire::encode_value(
                self.heap,
                self.execution,
                &val_rep,
                None,
                None,
                &mut self.foreign,
            ) {
                out.push(json!(["service", key, wire]));
            }
        }
        Ok(out)
    }

    fn extract_layer_entries(
        &self,
        root: &Value,
        out: &mut Vec<(String, Value)>,
    ) -> Result<(), Error> {
        let mut stack = vec![root.representation()];
        while let Some(cur) = stack.pop() {
            let Value::Object(tok) = cur else {
                continue;
            };
            match &self.heap.object(tok.id)?.data {
                Data::Context(map) => {
                    for (k, v) in map {
                        out.push((k.clone(), v.representation()));
                    }
                }
                Data::Layer(LayerData::Context(c)) => stack.push(c.representation()),
                Data::Layer(LayerData::Merge { left, right }) => {
                    stack.push(right.representation());
                    stack.push(left.representation());
                }
                Data::Layer(LayerData::Succeed {
                    tag,
                    implementation,
                }) => {
                    if let Ok(key) = self.service_tag_key(tag) {
                        out.push((key.to_owned(), implementation.representation()));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn foreign_build_layer(
        &mut self,
        layer: Value,
        execution: usize,
        host_id: u64,
        scope_owner: usize,
        or_die: bool,
        target: LayerBuildTarget,
    ) -> Result<(), Error> {
        if execution != self.execution {
            self.heap.drop_value(layer)?;
            return Err(Error::ExpectedLayer);
        }
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        let call_id = self.alloc_call_id();
        self.foreign.mirrored_owners.insert(scope_owner);
        let services = match self.collect_active_services_wire() {
            Ok(s) => s,
            Err(err) => {
                self.heap.drop_value(layer)?;
                return Err(err);
            }
        };
        let msg = json!({
            "op": "build_layer",
            "exec": self.execution,
            "call_id": call_id,
            "handle": host_id,
            "owner": scope_owner,
            "evidence_owner": scope_owner,
            "services": services,
        });
        if let Err(err) = self
            .bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)
        {
            self.heap.drop_value(layer)?;
            return Err(err);
        }
        // Release the layer handle strictly AFTER sending `build_layer`.
        self.heap.drop_value(layer)?;
        self.flush_released_handles()?;
        self.wait_for_call_response(
            call_id,
            Destination::ForeignLayerBuildFinish { or_die, target },
            None,
        )
    }

    fn foreign_lookup_service(&mut self, binding: Local, key: &str) -> Result<(), Error> {
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        let call_id = self.alloc_call_id();
        let (owner, ev_owner) = self.current_owner_context();
        let services = self.collect_active_services_wire()?;
        let msg = json!({
            "op": "lookup_service",
            "exec": self.execution,
            "call_id": call_id,
            "key": key,
            "owner": owner,
            "evidence_owner": ev_owner,
            "services": services,
        });
        self.bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)?;
        loop {
            let reply = self.bridge.as_deref_mut().expect("bridge opened").recv()?;
            match reply.get("type").and_then(Json::as_str) {
                Some("stdout") => {
                    if let Some(text) = reply.get("text").and_then(Json::as_str) {
                        self.stdout.push_str(text);
                    }
                }
                Some("call_done")
                    if reply.get("call_id").and_then(Json::as_u64) == Some(call_id) =>
                {
                    if reply.get("ok").and_then(Json::as_bool) == Some(true) {
                        let wire = reply.get("value").ok_or_else(|| {
                            Error::HostBridge("lookup_service missing value".into())
                        })?;
                        let val = wire::decode_value(self.heap, self.execution, wire)?;
                        return self.store(binding, val);
                    }
                    return self
                        .raise_defect(DefectSpec::custom(format!("Service not found: {key}")));
                }
                Some("call_done") => {
                    if let Some(cid) = reply.get("call_id").and_then(Json::as_u64) {
                        self.resolve_pending_async_call(cid, &reply)?;
                    }
                }
                Some("cb_invoke") => self.handle_cb_invoke(&reply)?,
                Some("cb_cancel") => self.handle_cb_cancel(&reply)?,
                Some("cell_get") => self.handle_cell_get(&reply)?,
                Some("cell_set") => self.handle_cell_set(&reply)?,
                Some("fatal") => {
                    let text = reply
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("worker fatal");
                    return Err(Error::HostBridge(text.to_owned()));
                }
                _ => {}
            }
        }
    }

    pub(super) fn foreign_perform_service(
        &mut self,
        binding: Local,
        effect: &str,
        op: &str,
    ) -> Result<(), Error> {
        self.foreign_service_call(binding, None, Some(effect), op)
    }

    pub(super) fn foreign_invoke_service_op(
        &mut self,
        binding: Local,
        execution: usize,
        host_id: u64,
        op: &str,
    ) -> Result<(), Error> {
        if execution != self.execution {
            return Err(Error::HostBridge("foreign service from another execution".into()));
        }
        self.foreign_service_call(binding, Some(host_id), None, op)
    }

    fn foreign_service_call(
        &mut self,
        binding: Local,
        handle: Option<u64>,
        effect: Option<&str>,
        op: &str,
    ) -> Result<(), Error> {
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        let mut encoded_args = Vec::with_capacity(self.scratch.len());
        for val in &self.scratch {
            encoded_args.push(wire::encode_value(
                self.heap,
                self.execution,
                val,
                None,
                None,
                &mut self.foreign,
            )?);
        }
        let call_id = self.alloc_call_id();
        let (owner, ev_owner) = self.current_owner_context();
        let services = self.collect_active_services_wire()?;
        let result_hint = match (effect, op) {
            (Some("FileSystem.FileSystem"), "stat" | "lstat") => json!({
                "Record": [
                    ["mode", "Int"],
                    ["uid", { "Option": "Int" }],
                    ["gid", { "Option": "Int" }],
                ]
            }),
            (Some("Terminal.Terminal" | "Cli.Prompt.Tty"), "columns" | "rows") => json!("Int"),
            (Some("Http.Router.RouteContext"), "params") => {
                json!({ "List": { "Tuple": ["String", "String"] } })
            }
            _ => json!("Any"),
        };
        let msg = json!({
            "op": "perform_service",
            "exec": self.execution,
            "call_id": call_id,
            "handle": handle,
            "effect": effect,
            "op_name": op,
            "args": encoded_args,
            "owner": owner,
            "evidence_owner": ev_owner,
            "services": services,
            "result_hint": result_hint,
        });
        self.bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)?;
        self.release_operands()?;
        self.flush_released_handles()?;
        self.wait_for_call_response(call_id, Destination::Bind(binding), None)
    }

    fn handle_cell_get(&mut self, msg: &Json) -> Result<(), Error> {
        let req_id = msg
            .get("req_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("cell_get missing req_id".into()))?;
        let cell_id = msg
            .get("cell_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("cell_get missing cell_id".into()))?;
        let Some(cell) = self.foreign.cells.get(&cell_id).map(|c| CellHandle {
            id: c.id,
            execution: c.execution,
        }) else {
            self.send_sync_cb_err(req_id, "unknown cell id");
            return Ok(());
        };
        let current = match self.heap.cell_contents(self.execution, &cell) {
            Ok(v) => v.representation(),
            Err(err) => {
                self.send_sync_cb_err(req_id, &err.to_string());
                return Ok(());
            }
        };
        let wire = wire::encode_value(
            self.heap,
            self.execution,
            &current,
            None,
            None,
            &mut self.foreign,
        )?;
        self.bridge.as_deref_mut().expect("bridge opened").send(&json!({
            "op": "cb_return",
            "exec": self.execution,
            "req_id": req_id,
            "ok": true,
            "value": wire,
        }))
    }

    fn handle_cell_set(&mut self, msg: &Json) -> Result<(), Error> {
        let req_id = msg
            .get("req_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("cell_set missing req_id".into()))?;
        let cell_id = msg
            .get("cell_id")
            .and_then(Json::as_u64)
            .ok_or_else(|| Error::HostBridge("cell_set missing cell_id".into()))?;
        let Some(cell) = self.foreign.cells.get(&cell_id).map(|c| CellHandle {
            id: c.id,
            execution: c.execution,
        }) else {
            self.send_sync_cb_err(req_id, "unknown cell id");
            return Ok(());
        };
        let wire = msg
            .get("value")
            .ok_or_else(|| Error::HostBridge("cell_set missing value".into()))?;
        let replacement = wire::decode_value(self.heap, self.execution, wire)?;
        let old = match self.heap.cell_contents_mut(self.execution, &cell) {
            Ok(slot) => mem::replace(slot, replacement),
            Err(err) => {
                let _ = self.heap.drop_value(replacement);
                self.send_sync_cb_err(req_id, &err.to_string());
                return Ok(());
            }
        };
        self.heap.drop_value(old)?;
        self.flush_released_handles()?;
        self.bridge.as_deref_mut().expect("bridge opened").send(&json!({
            "op": "cb_return",
            "exec": self.execution,
            "req_id": req_id,
            "ok": true,
            "value": { "root": 0, "nodes": [["u"]] },
        }))
    }

    /// Waits in real time for a bridge event up to the next Rust timer deadline so
    /// virtual timers do not preempt live real-world JS async I/O (`timeout_option`).
    fn poll_bridge_with_timer_budget(&mut self) -> Result<bool, Error> {
        let Some((&(deadline, _), _)) = self.timers.first_key_value() else {
            return Ok(false);
        };
        let remaining_ns = deadline.saturating_sub(self.virtual_now_ns);
        if remaining_ns == 0 {
            return self.fire_due_timers();
        }
        self.flush_released_handles()?;
        let cap_ns = remaining_ns.min(25_000_000);
        let started = StdInstant::now();
        let maybe_msg = self
            .bridge
            .as_deref_mut()
            .expect("bridge opened")
            .recv_timeout(StdDuration::from_nanos(cap_ns))?;
        let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(cap_ns);
        if let Some(msg) = maybe_msg {
            self.virtual_now_ns = self
                .virtual_now_ns
                .saturating_add(elapsed_ns.min(remaining_ns.saturating_sub(1)));
            self.dispatch_bridge_event(&msg)?;
            return Ok(true);
        }
        self.virtual_now_ns = self
            .virtual_now_ns
            .saturating_add(elapsed_ns.max(cap_ns));
        self.fire_due_timers()?;
        Ok(true)
    }
}

fn collect_pat_tags(pat: &Pat, out: &mut BTreeSet<String>) {
    let mut stack = vec![pat];
    while let Some(p) = stack.pop() {
        match p {
            Pat::Con(con, args) => {
                if !con.name.starts_with('$') {
                    out.insert(format!("{}.{}.{}", con.module, con.ty, con.name));
                }
                for a in args {
                    stack.push(a);
                }
            }
            Pat::Tuple(items) | Pat::Or(items) => {
                for i in items {
                    stack.push(i);
                }
            }
            Pat::List(head, rest) => {
                for i in head {
                    stack.push(i);
                }
                if let Some(r) = rest {
                    stack.push(r);
                }
            }
            Pat::Record(fields) => {
                for (_, fp) in fields {
                    stack.push(fp);
                }
            }
            _ => {}
        }
    }
}
