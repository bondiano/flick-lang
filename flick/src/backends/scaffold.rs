//! Lightweight WebAssembly backend loader for `flick new` and `flick init`
//! scaffolding, avoiding host tool probing (`node`) or project directory
//! inspection.

use flick_backend::command;
use flick_backend::host::runtime::Cancellation;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{Backend, BackendPin};
use flick_core::diagnostics::Diagnostic;

use super::{failure, options};
use crate::commands::contributed::Contributor;
use crate::component;

/// Lightweight component wrapper for scaffolding (`flick new` / `flick init`),
/// avoiding host tool probing (`node`) or project directory inspection.
pub(super) struct ScaffoldBackend(WasmBackend);

impl ScaffoldBackend {
    pub(super) fn from_bytes(
        pin: &BackendPin,
        bytes: &[u8],
        cache_descriptor: bool,
    ) -> Result<Self, Diagnostic> {
        let backend = WasmBackend::load_with_cancellation(
            component::engine().map_err(failure)?,
            pin.clone(),
            bytes,
            Budget::default(),
            component::cancellation().map_err(failure)?.clone(),
        )
        .map_err(|why| {
            failure(format!(
                "backend {}@{}#{}: {why}",
                pin.identity.id, pin.identity.version, pin.sha256
            ))
        })?;
        if cache_descriptor {
            options::save_descriptor(&pin.sha256, &backend.describe());
        }
        Ok(Self(backend))
    }
}

impl Contributor for ScaffoldBackend {
    fn describe(&self) -> Result<Vec<command::Descriptor>, String> {
        Ok(self.0.commands())
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::run(&self.0, invocation)
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::resume(&self.0, state, outputs, answers)
    }

    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        Backend::open(&self.0, invocation)
    }

    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: &[command::Answer],
        cancel: &Cancellation,
    ) -> Result<command::Handled, String> {
        self.0.handle_cancellable(&state, &request, answers, cancel)
    }

    fn close(&self, state: Vec<u8>) -> command::Plan {
        Backend::close(&self.0, state)
    }

    fn templates(&self) -> Result<Vec<command::TemplateDescriptor>, String> {
        self.0.try_templates()
    }

    fn scaffold(
        &self,
        request: &command::TemplateRequest,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        Backend::scaffold(&self.0, request)
    }
}
