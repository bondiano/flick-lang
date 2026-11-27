// Non-native / wasm32 stub for `ForeignState` and `HostBridge` hooks.
// Preserves capability checks identically while rejecting bridged foreign calls
// without pulling OS process/socket dependencies into wasm32 builds.

use flick_core::ir::memory::ForeignCallSpec;

#[derive(Default)]
struct ForeignState {
    bridge_opened: bool,
    pending_host_cbs: BTreeMap<usize, u64>,
    mirrored_owners: BTreeSet<usize>,
    sync_cb_result: Option<(u64, Result<Value, CauseToken>)>,
    sync_task_fiber: Option<usize>,
}

#[allow(
    clippy::unused_self,
    clippy::unnecessary_wraps,
    reason = "stub matches native foreign.rs method signatures"
)]
impl Machine<'_, '_> {
    fn foreign_call(
        &mut self,
        _binding: Local,
        spec: &ForeignCallSpec,
        _args: &[Local],
    ) -> Result<(), Error> {
        self.check_foreign_call_capability(spec)?;
        Err(Error::UnsupportedOperation(
            "foreign call requires native-host HostBridge",
        ))
    }

    fn foreign_call_handle(
        &mut self,
        _destination: Destination,
        execution: usize,
        _host_id: u64,
    ) -> Result<(), Error> {
        if execution != self.execution {
            return Err(Error::ForeignClosure);
        }
        self.check_ffi_capability("HostBridge.call_handle")?;
        Err(Error::UnsupportedOperation(
            "foreign handle calls require native-host HostBridge",
        ))
    }

    fn foreign_await_handle(
        &mut self,
        _destination: Destination,
        _host_id: u64,
    ) -> Result<(), Error> {
        self.check_ffi_capability("HostBridge.await_handle")?;
        Err(Error::UnsupportedOperation(
            "foreign handle await requires native-host HostBridge",
        ))
    }

    fn foreign_get_field(
        &mut self,
        _binding: Local,
        execution: usize,
        _host_id: u64,
        _field: &str,
    ) -> Result<(), Error> {
        if execution != self.execution {
            return Err(Error::HostBridge(
                "foreign handle from another execution".into(),
            ));
        }
        self.check_ffi_capability("HostBridge.get_field")?;
        Err(Error::UnsupportedOperation(
            "foreign handle field access requires native-host HostBridge",
        ))
    }

    fn foreign_build_layer(
        &mut self,
        layer: Value,
        _execution: usize,
        _host_id: u64,
        _scope_owner: usize,
        _or_die: bool,
        _target: LayerBuildTarget,
    ) -> Result<(), Error> {
        self.heap.drop_value(layer)?;
        Err(Error::UnsupportedOperation(
            "foreign host layers require native-host HostBridge",
        ))
    }

    fn foreign_lookup_service(&mut self, _binding: Local, _key: &str) -> Result<(), Error> {
        Err(Error::UnsupportedOperation(
            "foreign service lookup requires native-host HostBridge",
        ))
    }

    pub(super) fn foreign_perform_service(
        &mut self,
        _binding: Local,
        _effect: &str,
        _op: &str,
    ) -> Result<(), Error> {
        Err(Error::UnsupportedOperation(
            "foreign service calls require native-host HostBridge",
        ))
    }

    pub(super) fn foreign_invoke_service_op(
        &mut self,
        _binding: Local,
        _execution: usize,
        _host_id: u64,
        _op: &str,
    ) -> Result<(), Error> {
        Err(Error::UnsupportedOperation(
            "foreign service calls require native-host HostBridge",
        ))
    }

    fn wrap_host_context_handle(&mut self, handle: Value) -> Result<Value, Error> {
        stdlib::runtime::coop::wrap_host_context_handle(self.heap, handle)
    }

    fn has_pending_foreign_work(&self) -> bool {
        false
    }

    fn try_poll_bridge_nonblocking(&mut self) -> Result<bool, Error> {
        Ok(false)
    }

    fn poll_bridge_once(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn poll_bridge_with_timer_budget(&mut self) -> Result<bool, Error> {
        Ok(false)
    }

    fn try_foreign_handles_equal(&mut self) -> Result<Option<bool>, Error> {
        Ok(None)
    }

    fn close_mirrored_owner(&mut self, _owner: usize) -> Result<(), Error> {
        Ok(())
    }

    fn cleanup_foreign_roots(&mut self, _roots: &mut Vec<Value>) {}

    fn finish_foreign_bridge(&mut self) -> Result<(), Error> {
        let _ = self.heap.take_released_foreign_handles(self.execution);
        Ok(())
    }

    fn cancel_foreign_call(&mut self, _call_id: u64) -> Result<(), Error> {
        Ok(())
    }

    fn send_async_cb_return(&mut self, _req_id: u64, _fid: usize) -> Result<(), Error> {
        Ok(())
    }
}
