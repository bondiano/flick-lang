//! Persistent in-process VM session for interactive REPL execution.
//!
//! Holds one long-lived [`Heap`], accumulated memory-IR program with monotonic
//! `CodeId` remapping, persistent initialized globals/cells/lazies, `Host.live`
//! root services (`Σ₀`), session-wide `:handle` handlers, `:provide` layers,
//! and background jobs (`:bg`, `:jobs`, `:kill`).

mod eval;
pub mod format;
mod jobs;
mod program;
#[cfg(test)]
mod tests;

use std::mem;

use flick_core::ir::memory::Name;

pub use format::render_value;
pub use jobs::{JobId, JobInfo, JobStatus, KillOutcome};

use jobs::JobTable;
use program::SessionProgram;

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
    Fault {
        message: String,
        stdout: String,
    },
}

struct InstalledHandler {
    global: Name,
    closure: Value,
    kind: HandlerKind,
    handled_effects: Vec<String>,
}

struct InstalledLayer {
    global: Name,
    value: Value,
    provided_services: Vec<String>,
}

/// One live VM REPL session sharing a single counted [`Heap`] across phrases.
pub struct VmSession {
    heap: Heap,
    execution: usize,
    program: SessionProgram,
    persistent: SessionPersistentState,
    root_layer: Option<Value>,
    root_layer_name: Option<Name>,
    layers: Vec<InstalledLayer>,
    handlers: Vec<InstalledHandler>,
    jobs: JobTable,
    host_mode: HostMode,
    fuel: u64,
    shutdown_done: bool,
}

impl Default for VmSession {
    fn default() -> Self {
        Self::new()
    }
}

impl VmSession {
    /// Creates a new live VM session using [`HostMode::LiveOs`].
    pub fn new() -> Self {
        Self::with_host_mode(HostMode::LiveOs)
    }

    /// Creates a new live VM session with an explicit [`HostMode`].
    pub fn with_host_mode(host_mode: HostMode) -> Self {
        Self {
            heap: Heap::new(),
            execution: allocate_execution_id(),
            program: SessionProgram::new(),
            persistent: SessionPersistentState::default(),
            root_layer: None,
            root_layer_name: None,
            layers: Vec::new(),
            handlers: Vec::new(),
            jobs: JobTable::default(),
            host_mode,
            fuel: u64::MAX,
            shutdown_done: false,
        }
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
