//! Backend-neutral REPL engine interface.
//!
//! Keeps the line loop and session commands independent of any execution
//! backend so VM and future JS/Lua/Go engines plug in behind one trait.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Structured outcome returned by a [`ReplEngine`] for one command or phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineResponse {
    /// Successful evaluation or command, with optional captured program stdout
    /// and optional formatted result line (such as `42 : Int`).
    Output {
        stdout: String,
        result: Option<String>,
    },
    /// A task failed with a typed Flick error; the session stays alive.
    TypedError { stdout: String, message: String },
    /// An expression performed an unhandled user effect.
    UnhandledEffect {
        stdout: String,
        effect: String,
        hint: String,
    },
    /// Evaluation was interrupted (Ctrl-C / `SIGINT`); scopes finalized cleanly.
    Interrupted { stdout: String },
    /// Compile diagnostic, invalid command, or runtime fault.
    Error { stdout: String, message: String },
}

/// Shared interface for REPL execution backends.
pub trait ReplEngine {
    /// Evaluates one Flick declaration or expression phrase.
    fn eval(&mut self, input: &str, interrupt: Arc<AtomicBool>) -> EngineResponse;

    /// Installs a session-wide effect handler (`:handle <expr>`).
    fn handle(&mut self, expr: &str, interrupt: Arc<AtomicBool>) -> EngineResponse;

    /// Installs a session-wide service layer (`:provide <expr>`).
    fn provide(&mut self, expr: &str, interrupt: Arc<AtomicBool>) -> EngineResponse;

    /// Spawns a background task (`:bg <expr>`) and returns immediately.
    fn spawn_bg(&mut self, expr: &str) -> EngineResponse;

    /// Formats the currently tracked background jobs (`:jobs`).
    fn list_jobs(&self) -> Vec<String>;

    /// Stops background job `id` with scope finalization (`:kill <id>`).
    fn kill_job(&mut self, id: u64) -> EngineResponse;

    /// Stops any running background jobs and verifies clean teardown.
    fn shutdown(&mut self) -> Result<(), String>;
}
