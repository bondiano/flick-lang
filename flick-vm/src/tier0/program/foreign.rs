// Counted VM integration for explicitly bridged foreign calls (`HostBridge`).
// Owns all callback roots, pending async calls, mirrored owners, and handle flushes.

use flick_core::ir::memory::ForeignCallSpec;
use serde_json::{Value as Json, json};

use crate::stdlib::host_bridge::wire::{self, WireRegistrar};

struct PendingForeignCall<'p> {
    fiber: usize,
    destination: Destination<'p>,
    catch_fn: Option<Value>,
}

struct ForeignState<'p> {
    bridge_opened: bool,
    next_call_id: u64,
    next_cb_id: u64,
    next_cell_id: u64,
    callbacks: BTreeMap<u64, Value>,
    cb_by_obj: BTreeMap<(ObjectId, usize), u64>,
    dict_getters: BTreeMap<u64, (Value, Value)>,
    dict_getter_by_obj: BTreeMap<(ObjectId, ObjectId), u64>,
    cells: BTreeMap<u64, CellHandle>,
    cell_by_id: BTreeMap<ObjectId, u64>,
    pending_calls: BTreeMap<u64, PendingForeignCall<'p>>,
    pending_host_cbs: BTreeMap<usize, u64>,
    mirrored_owners: BTreeSet<usize>,
    closed_mirrored_owners: BTreeSet<usize>,
    active_sync_calls: BTreeSet<u64>,
    pending_sync_calls: BTreeSet<u64>,
    completed_sync_calls: BTreeMap<u64, Json>,
    sync_cb_result: Option<(u64, Result<Value, CauseToken>)>,
    sync_task_fiber: Option<usize>,
}

impl Default for ForeignState<'_> {
    fn default() -> Self {
        Self {
            bridge_opened: false,
            next_call_id: 1,
            next_cb_id: 1,
            next_cell_id: 1,
            callbacks: BTreeMap::new(),
            cb_by_obj: BTreeMap::new(),
            dict_getters: BTreeMap::new(),
            dict_getter_by_obj: BTreeMap::new(),
            cells: BTreeMap::new(),
            cell_by_id: BTreeMap::new(),
            pending_calls: BTreeMap::new(),
            pending_host_cbs: BTreeMap::new(),
            mirrored_owners: BTreeSet::new(),
            closed_mirrored_owners: BTreeSet::new(),
            active_sync_calls: BTreeSet::new(),
            pending_sync_calls: BTreeSet::new(),
            completed_sync_calls: BTreeMap::new(),
            sync_cb_result: None,
            sync_task_fiber: None,
        }
    }
}

impl WireRegistrar for ForeignState<'_> {
    fn register_closure(
        &mut self,
        heap: &mut Heap,
        id: ObjectId,
        arity: usize,
    ) -> Result<u64, Error> {
        if let Some(&existing) = self.cb_by_obj.get(&(id, arity)) {
            return Ok(existing);
        }
        let val = heap.dup(&Value::Object(ObjectToken { id }))?;
        let cb_id = self.next_cb_id;
        self.next_cb_id += 1;
        self.callbacks.insert(cb_id, val);
        self.cb_by_obj.insert((id, arity), cb_id);
        Ok(cb_id)
    }

    fn register_task(&mut self, heap: &mut Heap, id: ObjectId) -> Result<u64, Error> {
        if let Some(&existing) = self.cb_by_obj.get(&(id, usize::MAX)) {
            return Ok(existing);
        }
        let val = heap.dup(&Value::Object(ObjectToken { id }))?;
        let cb_id = self.next_cb_id;
        self.next_cb_id += 1;
        self.callbacks.insert(cb_id, val);
        self.cb_by_obj.insert((id, usize::MAX), cb_id);
        Ok(cb_id)
    }

    fn register_dict_getter(
        &mut self,
        heap: &mut Heap,
        dict_id: ObjectId,
        getter_id: ObjectId,
    ) -> Result<u64, Error> {
        if let Some(&existing) = self.dict_getter_by_obj.get(&(dict_id, getter_id)) {
            return Ok(existing);
        }
        let dict_val = heap.dup(&Value::Object(ObjectToken { id: dict_id }))?;
        let getter_val = match heap.dup(&Value::Object(ObjectToken { id: getter_id })) {
            Ok(v) => v,
            Err(err) => {
                let _ = heap.drop_value(dict_val);
                return Err(err);
            }
        };
        let cb_id = self.next_cb_id;
        self.next_cb_id += 1;
        self.dict_getters.insert(cb_id, (dict_val, getter_val));
        self.dict_getter_by_obj.insert((dict_id, getter_id), cb_id);
        Ok(cb_id)
    }

    fn register_cell(&mut self, cell: &CellHandle) -> u64 {
        if let Some(&existing) = self.cell_by_id.get(&cell.id) {
            return existing;
        }
        let cid = self.next_cell_id;
        self.next_cell_id += 1;
        self.cells.insert(
            cid,
            CellHandle {
                id: cell.id,
                execution: cell.execution,
            },
        );
        self.cell_by_id.insert(cell.id, cid);
        cid
    }

    fn record_owner(&mut self, owner_index: usize) {
        self.mirrored_owners.insert(owner_index);
    }
}

impl<'p> Machine<'p, '_> {
    fn ensure_bridge_open(&mut self) -> Result<(), Error> {
        if self.foreign.bridge_opened {
            return Ok(());
        }
        let exec = self.execution;
        let (tags, imports) = self.collect_program_metadata();
        let bridge = self
            .bridge
            .as_deref_mut()
            .ok_or(Error::UnsupportedOperation("foreign call requires HostBridge"))?;
        bridge.open_exec(exec, &tags, &imports)?;
        self.foreign.bridge_opened = true;
        Ok(())
    }

    fn flush_released_handles(&mut self) -> Result<(), Error> {
        if !self.foreign.bridge_opened {
            return Ok(());
        }
        let ids = self.heap.take_released_foreign_handles(self.execution);
        if ids.is_empty() {
            return Ok(());
        }
        if let Some(bridge) = self.bridge.as_deref_mut() {
            bridge.release_handles(self.execution, &ids)?;
        }
        Ok(())
    }

    fn alloc_call_id(&mut self) -> u64 {
        let id = self.foreign.next_call_id;
        self.foreign.next_call_id += 1;
        id
    }

    fn current_owner_context(&mut self) -> (usize, Option<usize>) {
        let owner = self.current_owner;
        self.foreign.mirrored_owners.insert(owner);
        let ev = self.current_evidence.map(|e| e.index);
        if let Some(ev_idx) = ev {
            self.foreign.mirrored_owners.insert(ev_idx);
        }
        (owner, ev)
    }

    fn foreign_call(
        &mut self,
        binding: Local,
        spec: &ForeignCallSpec,
        args: &[Local],
    ) -> Result<(), Error> {
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        self.stage(args.iter().copied())?;
        let catch_fn = if spec.has_catch {
            self.scratch.pop()
        } else {
            None
        };
        let mut encoded_args = Vec::with_capacity(self.scratch.len());
        for (i, val) in self.scratch.iter().enumerate() {
            let (shape, cb_arity) = if i >= spec.dicts {
                let arg_i = i - spec.dicts;
                (
                    spec.adapter.shapes.get(arg_i),
                    spec.adapter.callbacks.get(arg_i).copied().flatten(),
                )
            } else {
                (None, None)
            };
            match wire::encode_value(
                self.heap,
                self.execution,
                val,
                shape,
                cb_arity,
                &mut self.foreign,
            ) {
                Ok(w) => encoded_args.push(w),
                Err(err) => {
                    if let Some(cf) = catch_fn {
                        let _ = self.heap.drop_value(cf);
                    }
                    return Err(err);
                }
            }
        }
        let call_id = self.alloc_call_id();
        let (owner, ev_owner) = self.current_owner_context();
        let services = match self.collect_active_services_wire() {
            Ok(s) => s,
            Err(err) => {
                if let Some(cf) = catch_fn {
                    let _ = self.heap.drop_value(cf);
                }
                return Err(err);
            }
        };
        let spec_json = serde_json::to_value(spec)
            .map_err(|e| Error::HostBridge(format!("serialize ForeignCallSpec: {e}")))?;
        let msg = json!({
            "op": "call",
            "exec": self.execution,
            "call_id": call_id,
            "spec": spec_json,
            "args": encoded_args,
            "owner": owner,
            "evidence_owner": ev_owner,
            "services": services,
        });
        if let Err(err) = self
            .bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)
        {
            if let Some(cf) = catch_fn {
                let _ = self.heap.drop_value(cf);
            }
            return Err(err);
        }
        // Release operands and flush freed handle IDs strictly AFTER sending `msg`
        // so TCP FIFO guarantees the worker decodes handle arguments before releasing them.
        self.release_operands()?;
        self.flush_released_handles()?;
        self.wait_for_call_response(call_id, Destination::Bind(binding), catch_fn)
    }

    fn foreign_call_handle(
        &mut self,
        destination: Destination<'p>,
        execution: usize,
        host_id: u64,
    ) -> Result<(), Error> {
        if execution != self.execution {
            return Err(Error::ForeignClosure);
        }
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        let arg_wire = wire::encode_value(
            self.heap,
            self.execution,
            &self.scratch[1],
            None,
            None,
            &mut self.foreign,
        )?;
        let call_id = self.alloc_call_id();
        let (owner, ev_owner) = self.current_owner_context();
        let services = self.collect_active_services_wire()?;
        let msg = json!({
            "op": "call_handle",
            "exec": self.execution,
            "call_id": call_id,
            "handle": host_id,
            "arg": arg_wire,
            "owner": owner,
            "evidence_owner": ev_owner,
            "services": services,
        });
        self.bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)?;
        self.release_operands()?;
        self.flush_released_handles()?;
        self.wait_for_call_response(call_id, destination, None)
    }

    fn foreign_await_handle(
        &mut self,
        destination: Destination<'p>,
        host_id: u64,
    ) -> Result<(), Error> {
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        let call_id = self.alloc_call_id();
        let (owner, ev_owner) = self.current_owner_context();
        let services = self.collect_active_services_wire()?;
        let msg = json!({
            "op": "await_handle",
            "exec": self.execution,
            "call_id": call_id,
            "handle": host_id,
            "owner": owner,
            "evidence_owner": ev_owner,
            "services": services,
        });
        self.bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)?;
        self.release_operands()?;
        self.flush_released_handles()?;
        self.wait_for_call_response(call_id, destination, None)
    }

    fn foreign_get_field(
        &mut self,
        binding: Local,
        execution: usize,
        host_id: u64,
        field: &str,
    ) -> Result<(), Error> {
        if execution != self.execution {
            return Err(Error::HostBridge(
                "foreign handle from another execution".into(),
            ));
        }
        self.ensure_bridge_open()?;
        self.flush_released_handles()?;
        let call_id = self.alloc_call_id();
        let msg = json!({
            "op": "get_field",
            "exec": self.execution,
            "call_id": call_id,
            "handle": host_id,
            "field": field,
        });
        self.bridge
            .as_deref_mut()
            .expect("bridge opened")
            .send(&msg)?;
        self.wait_for_call_response(call_id, Destination::Bind(binding), None)
    }
}
