//! Single-threaded stub for background jobs on `wasm32` / without `native-host`.

use flick_core::ir::memory::{Name, Program};

use super::VmSession;

/// Monotonic background job identifier (`1, 2, ...`).
pub type JobId = u64;

/// Lifecycle status of a background job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Done(String),
    Failed(String),
    Killed,
}

impl JobStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done(_) => "done",
            Self::Failed(_) => "failed",
            Self::Killed => "killed",
        }
    }
}

/// Snapshot of a background job for `:jobs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInfo {
    pub id: JobId,
    pub label: String,
    pub status: JobStatus,
}

/// Result of stopping a job via `:kill <id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillOutcome {
    pub id: JobId,
    pub status: JobStatus,
    pub stdout: String,
    pub leak_error: Option<String>,
}

pub(super) struct JobTable;

#[allow(
    clippy::unused_self,
    clippy::unnecessary_wraps,
    reason = "stub matches native jobs.rs method signatures"
)]
impl JobTable {
    pub(super) const fn new() -> Self {
        Self
    }

    pub(super) fn list(&self) -> Vec<JobInfo> {
        Vec::new()
    }

    pub(super) fn kill(&mut self, id: JobId) -> Result<KillOutcome, String> {
        Err(format!("no background job `{id}`"))
    }

    pub(super) fn kill_all(&mut self) -> Result<(), String> {
        Ok(())
    }
}

impl VmSession {
    /// Background thread spawning is unavailable without `native-host`.
    pub fn spawn_bg(
        &mut self,
        _incoming: &Program,
        _entry: &Name,
        _root: Option<&Name>,
        _label: String,
    ) -> Result<JobId, String> {
        Err("background threads require native-host; use start_eval/step on wasm32".to_owned())
    }

    /// Lists all tracked background jobs (always empty without `native-host`).
    pub fn list_jobs(&self) -> Vec<JobInfo> {
        self.jobs.list()
    }

    /// Interrupts background job `id` (unavailable without `native-host`).
    pub fn kill_job(&mut self, id: JobId) -> Result<KillOutcome, String> {
        self.jobs.kill(id)
    }
}
