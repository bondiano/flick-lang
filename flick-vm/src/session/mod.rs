//! Persistent in-process VM session for interactive REPL execution.
//!
//! Holds one long-lived [`Heap`], accumulated memory-IR program with monotonic
//! `CodeId` remapping, persistent initialized globals/cells/lazies, `Host.live`
//! root services (`Σ₀`), session-wide `:handle` handlers, `:provide` layers,
//! and background jobs (`:bg`, `:jobs`, `:kill`).

mod eval;
pub mod format;
mod jobs;
pub(crate) mod program;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use flick_core::iface::Interface;
use flick_core::ir::memory::{MemorySpan, Name};

pub use format::render_value;
pub use jobs::{JobId, JobInfo, JobStatus, KillOutcome};

use jobs::JobTable;
pub(crate) use program::SessionProgram;

use crate::image::{
    self, HeapInventory, ImageError, LoadReport, SaveSelection, UnloadReport, VmImage,
};
use crate::sandbox::{Capability, CapabilitySet, LimitKind, ResourceLimits, SandboxConfig};
use crate::tier0::{
    Error, Heap, HostMode, SessionHandlerKind, SessionPersistentState, Value,
    allocate_execution_id, shutdown_persistent_state,
};

/// Calling convention of a `:handle` closure installed on [`VmSession`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    Thunk { returns_task: bool },
    Task { returns_task: bool },
}

impl From<HandlerKind> for SessionHandlerKind {
    fn from(kind: HandlerKind) -> Self {
        match kind {
            HandlerKind::Thunk { returns_task } => Self::Thunk { returns_task },
            HandlerKind::Task { returns_task } => Self::Task { returns_task },
        }
    }
}

/// Structured outcome of evaluating a REPL phrase or session command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalOutcome {
    Value {
        rendered: String,
        stdout: String,
    },
    Declaration {
        stdout: String,
    },
    TypedError {
        rendered: String,
        stdout: String,
    },
    UnhandledEffect {
        effect: String,
        op: String,
        stdout: String,
    },
    Interrupted {
        stdout: String,
    },
    CapabilityDenied {
        capability: Capability,
        binding: String,
        span: Option<MemorySpan>,
        stdout: String,
    },
    LimitExceeded {
        kind: LimitKind,
        message: String,
        stdout: String,
    },
    Fault {
        message: String,
        stdout: String,
    },
}

pub(crate) struct InstalledHandler {
    pub(crate) global: Name,
    pub(crate) closure: Value,
    pub(crate) kind: HandlerKind,
    pub(crate) handled_effects: Vec<String>,
}

pub(crate) struct InstalledLayer {
    pub(crate) global: Name,
    pub(crate) value: Value,
    pub(crate) provided_services: Vec<String>,
}

/// One live VM REPL session sharing a single counted [`Heap`] across phrases.
pub struct VmSession {
    pub(crate) heap: Heap,
    pub(crate) execution: usize,
    pub(crate) program: SessionProgram,
    pub(crate) persistent: SessionPersistentState,
    pub(crate) modules: BTreeMap<String, Interface>,
    pub(crate) retired_cells: Vec<(String, Value)>,
    pub(crate) retired_lazies: Vec<(String, Value)>,
    pub(crate) root_layer: Option<Value>,
    pub(crate) root_layer_name: Option<Name>,
    pub(crate) layers: Vec<InstalledLayer>,
    pub(crate) handlers: Vec<InstalledHandler>,
    jobs: JobTable,
    host_mode: HostMode,
    fuel: u64,
    sandbox: SandboxConfig,
    shutdown_done: bool,
}

impl Default for VmSession {
    fn default() -> Self {
        Self::new()
    }
}

impl VmSession {
    /// Creates a new live VM session using [`HostMode::LiveOs`] and unrestricted capabilities.
    pub fn new() -> Self {
        Self::with_host_mode(HostMode::LiveOs)
    }

    /// Creates a new live VM session with an explicit [`HostMode`] and unrestricted capabilities.
    pub fn with_host_mode(host_mode: HostMode) -> Self {
        Self::with_sandbox(host_mode, SandboxConfig::unrestricted())
    }

    /// Creates a sandboxed VM session using [`HostMode::LiveOs`] and `sandbox`.
    pub fn sandboxed(sandbox: SandboxConfig) -> Self {
        Self::with_sandbox(HostMode::LiveOs, sandbox)
    }

    /// Creates a live VM session with an explicit [`HostMode`] and [`SandboxConfig`].
    pub fn with_sandbox(host_mode: HostMode, sandbox: SandboxConfig) -> Self {
        let fuel = sandbox.limits.fuel.unwrap_or(u64::MAX);
        Self {
            heap: Heap::new(),
            execution: allocate_execution_id(),
            program: SessionProgram::new(),
            persistent: SessionPersistentState::default(),
            modules: BTreeMap::new(),
            retired_cells: Vec::new(),
            retired_lazies: Vec::new(),
            root_layer: None,
            root_layer_name: None,
            layers: Vec::new(),
            handlers: Vec::new(),
            jobs: JobTable::default(),
            host_mode,
            fuel,
            sandbox,
            shutdown_done: false,
        }
    }

    /// Returns the active [`SandboxConfig`] for this session.
    #[must_use]
    pub const fn sandbox(&self) -> &SandboxConfig {
        &self.sandbox
    }

    /// Replaces the active [`SandboxConfig`] for subsequent evaluations and loads.
    pub fn set_sandbox(&mut self, sandbox: SandboxConfig) {
        self.fuel = sandbox.limits.fuel.unwrap_or(u64::MAX);
        self.sandbox = sandbox;
    }

    /// Returns the granted [`CapabilitySet`] for this session.
    #[must_use]
    pub const fn capabilities(&self) -> CapabilitySet {
        self.sandbox.capabilities
    }

    /// Updates the granted [`CapabilitySet`] for subsequent link and runtime checks.
    pub fn set_capabilities(&mut self, capabilities: CapabilitySet) {
        self.sandbox.capabilities = capabilities;
    }

    /// Returns the active [`ResourceLimits`] for this session.
    #[must_use]
    pub const fn limits(&self) -> &ResourceLimits {
        &self.sandbox.limits
    }

    /// Updates the active [`ResourceLimits`] for subsequent evaluations.
    pub fn set_limits(&mut self, limits: ResourceLimits) {
        self.fuel = limits.fuel.unwrap_or(u64::MAX);
        self.sandbox.limits = limits;
    }

    /// Returns the live accounted heap bytes in this session.
    #[must_use]
    pub const fn heap_live_bytes(&self) -> usize {
        self.heap.live_bytes()
    }

    /// Registers the closed type [`Interface`] for a loaded module.
    pub fn register_module_interface(&mut self, interface: Interface) {
        self.modules.insert(interface.module.clone(), interface);
    }

    /// Returns the registered [`Interface`] for `module`, if loaded.
    #[must_use]
    pub fn module_interface(&self, module: &str) -> Option<&Interface> {
        self.modules.get(module)
    }

    /// Returns the names of all modules currently present in this session
    /// (registered interfaces and modules with live globals).
    #[must_use]
    pub fn loaded_modules(&self) -> BTreeSet<String> {
        let mut names: BTreeSet<String> = self.modules.keys().cloned().collect();
        for g in &self.program.program().globals {
            names.insert(g.name.module.clone());
        }
        names
    }

    /// Saves the entire live session into a binary-encodable [`VmImage`].
    pub fn save_image(&self, entry: Option<Name>) -> Result<VmImage, ImageError> {
        image::save_session_image(self, entry)
    }

    /// Saves selected modules (or the whole session) into a [`VmImage`].
    pub fn save_modules(
        &self,
        selection: SaveSelection<'_>,
        entry: Option<Name>,
    ) -> Result<VmImage, ImageError> {
        image::save_session_modules(self, selection, entry)
    }

    /// Loads and links a decoded [`VmImage`] into this session.
    pub fn load_image(&mut self, image: &VmImage) -> Result<LoadReport, ImageError> {
        image::load_image_into_session(self, image)
    }

    /// Unloads `module` from this live session, returning a report of dependents
    /// and remaining live heap slots still referenced by surviving closures.
    pub fn unload_module(&mut self, module: &str) -> Result<UnloadReport, ImageError> {
        image::unload_module(self, module)
    }

    /// Reclaims retired cells, lazies, and code bodies of previously unloaded
    /// modules once no live session root or closure references them.
    pub fn reclaim_unloaded(&mut self) -> Result<(), ImageError> {
        image::reclaim_unloaded(self)
    }

    /// Returns the per-module live heap-slot inventory for this session.
    #[must_use]
    pub fn heap_inventory(&self) -> HeapInventory {
        image::session_heap_inventory(self)
    }

    /// Lists effect names currently handled by installed `:handle` wrappers.
    pub fn handled_effects(&self) -> Vec<String> {
        self.handlers
            .iter()
            .flat_map(|h| h.handled_effects.iter().cloned())
            .collect()
    }

    /// Lists service names currently provided by installed `:provide` layers.
    pub fn provided_services(&self) -> Vec<String> {
        self.layers
            .iter()
            .flat_map(|l| l.provided_services.iter().cloned())
            .collect()
    }

    /// Stops all running background jobs, releases all persistent heap roots,
    /// and verifies zero leaked heap slots.
    pub fn shutdown(&mut self) -> Result<(), String> {
        if self.shutdown_done {
            return Ok(());
        }
        self.shutdown_done = true;
        let bg_res = self.jobs.kill_all();
        let mut heap_err: Option<Error> = None;
        for handler in mem::take(&mut self.handlers) {
            if let Err(err) = self.heap.drop_value(handler.closure) {
                heap_err.get_or_insert(err);
            }
        }
        for layer in mem::take(&mut self.layers) {
            if let Err(err) = self.heap.drop_value(layer.value) {
                heap_err.get_or_insert(err);
            }
        }
        if let Some(root) = self.root_layer.take()
            && let Err(err) = self.heap.drop_value(root)
        {
            heap_err.get_or_insert(err);
        }
        // Move any still-retired cells/lazies back into persistent state so
        // `shutdown_persistent_state` detaches and releases them cleanly.
        for (_, cell) in mem::take(&mut self.retired_cells) {
            self.persistent.cells.push(cell);
        }
        for (_, lazy) in mem::take(&mut self.retired_lazies) {
            self.persistent.lazies.push(lazy);
        }
        if let Err(err) = shutdown_persistent_state(&mut self.heap, &mut self.persistent) {
            heap_err.get_or_insert(err);
        }
        let finish_res = self.heap.finish().map(|_| ());
        bg_res?;
        if let Some(err) = heap_err {
            return Err(err.to_string());
        }
        finish_res.map_err(|err| err.to_string())
    }
}

impl Drop for VmSession {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
