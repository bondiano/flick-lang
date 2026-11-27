//! Background job management (`:bg`, `:jobs`, `:kill`) for [`VmSession`].

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use flick_core::ir::memory::{Name, Program};

use super::format::{describe_cause, extract_unhandled_effect, is_interrupt_cause, render_value};
use super::program::SessionProgram;
use super::{HandlerKind, VmSession};
use crate::tier0::{
    Heap, HostMode, ProgramOutcome, SessionHandlerKind, SessionPersistentState, SessionRunSpec,
    Value, allocate_execution_id, build_session_layer_value, execute_session_step,
    shutdown_persistent_state,
};

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

struct JobShared {
    status: JobStatus,
    stdout: String,
    leak_error: Option<String>,
}

pub(super) struct BgJob {
    id: JobId,
    label: String,
    interrupt: Arc<AtomicBool>,
    shared: Arc<Mutex<JobShared>>,
    thread: Option<JoinHandle<()>>,
}

struct BgRunInput {
    program: Program,
    entry: Name,
    root_layer_name: Option<Name>,
    layer_globals: Vec<Name>,
    handler_globals: Vec<(Name, SessionHandlerKind)>,
    fuel: u64,
    host_mode: HostMode,
    interrupt: Arc<AtomicBool>,
    shared: Arc<Mutex<JobShared>>,
}

#[derive(Default)]
pub(super) struct JobTable {
    next_id: JobId,
    jobs: BTreeMap<JobId, BgJob>,
}

impl JobTable {
    pub(super) fn list(&self) -> Vec<JobInfo> {
        self.jobs
            .values()
            .map(|job| {
                let status = job.shared.lock().map_or_else(
                    |_| JobStatus::Failed("job state poisoned".to_owned()),
                    |s| s.status.clone(),
                );
                JobInfo {
                    id: job.id,
                    label: job.label.clone(),
                    status,
                }
            })
            .collect()
    }

    pub(super) fn kill(&mut self, id: JobId) -> Result<KillOutcome, String> {
        let Some(job) = self.jobs.get_mut(&id) else {
            return Err(format!("no background job `{id}`"));
        };
        job.interrupt.store(true, Ordering::Release);
        if let Some(handle) = job.thread.take() {
            let _ = handle.join();
        }
        let guard = job
            .shared
            .lock()
            .map_err(|_| "background job lock poisoned".to_owned())?;
        Ok(KillOutcome {
            id,
            status: guard.status.clone(),
            stdout: guard.stdout.clone(),
            leak_error: guard.leak_error.clone(),
        })
    }

    pub(super) fn kill_all(&mut self) -> Result<(), String> {
        let ids: Vec<JobId> = self.jobs.keys().copied().collect();
        let mut first_leak = None;
        for id in ids {
            let outcome = self.kill(id)?;
            if let Some(leak) = outcome.leak_error {
                first_leak.get_or_insert(leak);
            }
        }
        match first_leak {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

impl VmSession {
    /// Spawns a background task (`:bg <expr>`) on a dedicated thread with its own
    /// interrupt flag and returns its [`JobId`] immediately.
    pub fn spawn_bg(
        &mut self,
        incoming: &Program,
        entry: &Name,
        root: Option<&Name>,
        label: String,
    ) -> Result<JobId, String> {
        self.program.load(incoming);
        if let Some(err) = self.ensure_root_layer(root, None) {
            return Err(format!("failed to initialize host layer: {err:?}"));
        }
        let id = self.jobs.next_id + 1;
        self.jobs.next_id = id;
        let interrupt = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(JobShared {
            status: JobStatus::Running,
            stdout: String::new(),
            leak_error: None,
        }));
        let input = BgRunInput {
            program: self.program.program().clone(),
            entry: entry.clone(),
            root_layer_name: self.root_layer_name.clone().or_else(|| root.cloned()),
            layer_globals: self.layers.iter().map(|l| l.global.clone()).collect(),
            handler_globals: self
                .handlers
                .iter()
                .map(|h| (h.global.clone(), HandlerKind::into(h.kind)))
                .collect(),
            fuel: self.fuel,
            host_mode: self.host_mode,
            interrupt: Arc::clone(&interrupt),
            shared: Arc::clone(&shared),
        };
        let thread = thread::Builder::new()
            .name(format!("flick-repl-bg-{id}"))
            .spawn(move || run_bg_thread(&input))
            .map_err(|e| format!("failed to spawn background job thread: {e}"))?;
        self.jobs.jobs.insert(
            id,
            BgJob {
                id,
                label,
                interrupt,
                shared,
                thread: Some(thread),
            },
        );
        Ok(id)
    }

    /// Lists all tracked background jobs and their live status.
    pub fn list_jobs(&self) -> Vec<JobInfo> {
        self.jobs.list()
    }

    /// Interrupts background job `id` and waits for its LIFO scope finalizers to finish.
    pub fn kill_job(&mut self, id: JobId) -> Result<KillOutcome, String> {
        self.jobs.kill(id)
    }
}

fn run_bg_thread(input: &BgRunInput) {
    let mut heap = Heap::new();
    let execution = allocate_execution_id();
    let mut persistent = SessionPersistentState::default();
    let mut stdout = String::new();
    let mut root_layer: Option<Value> = None;
    let mut provided_layers: Vec<Value> = Vec::new();
    let mut handlers: Vec<(Value, SessionHandlerKind)> = Vec::new();
    let status = execute_bg_job(
        input,
        &mut heap,
        execution,
        &mut persistent,
        &mut stdout,
        &mut root_layer,
        &mut provided_layers,
        &mut handlers,
    );

    // Release all owned heap roots and verify zero leaks on background exit.
    let mut teardown_err: Option<String> = None;
    for (closure, _) in handlers {
        if let Err(err) = heap.drop_value(closure) {
            teardown_err.get_or_insert_with(|| err.to_string());
        }
    }
    for layer in provided_layers {
        if let Err(err) = heap.drop_value(layer) {
            teardown_err.get_or_insert_with(|| err.to_string());
        }
    }
    if let Some(root) = root_layer
        && let Err(err) = heap.drop_value(root)
    {
        teardown_err.get_or_insert_with(|| err.to_string());
    }
    if let Err(err) = shutdown_persistent_state(&mut heap, &mut persistent) {
        teardown_err.get_or_insert_with(|| err.to_string());
    }
    if let Err(leak) = heap.finish() {
        teardown_err.get_or_insert_with(|| leak.to_string());
    }
    if let Ok(mut guard) = input.shared.lock() {
        guard.status = status;
        guard.stdout = stdout;
        guard.leak_error = teardown_err;
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "isolates background job execution from heap teardown"
)]
fn execute_bg_job(
    input: &BgRunInput,
    heap: &mut Heap,
    execution: usize,
    persistent: &mut SessionPersistentState,
    stdout: &mut String,
    root_layer: &mut Option<Value>,
    provided_layers: &mut Vec<Value>,
    handlers: &mut Vec<(Value, SessionHandlerKind)>,
) -> JobStatus {
    let codes = SessionProgram::synthetic_codes();
    if let Some(root_name) = &input.root_layer_name
        && input.program.globals.iter().any(|g| &g.name == root_name)
    {
        let spec = SessionRunSpec {
            entry: root_name,
            await_task: false,
            root_layer: None,
            provided_layers: &[],
            handlers: &[],
            fuel: input.fuel,
            host_mode: input.host_mode,
            external_interrupt: Some(Arc::clone(&input.interrupt)),
            synthetic_codes: codes,
        };
        let report = execute_session_step(heap, &input.program, execution, persistent, &spec);
        stdout.push_str(&report.stdout);
        match report.outcome {
            ProgramOutcome::Returned(layer) => *root_layer = Some(layer),
            other => return classify_bg_failure(heap, other),
        }
    }
    for layer_name in &input.layer_globals {
        let reps: Vec<Value> = provided_layers.iter().map(Value::representation).collect();
        let spec = SessionRunSpec {
            entry: layer_name,
            await_task: false,
            root_layer: root_layer.as_ref(),
            provided_layers: &reps,
            handlers: &[],
            fuel: input.fuel,
            host_mode: input.host_mode,
            external_interrupt: Some(Arc::clone(&input.interrupt)),
            synthetic_codes: codes,
        };
        let report = build_session_layer_value(heap, &input.program, execution, persistent, &spec);
        stdout.push_str(&report.stdout);
        match report.outcome {
            ProgramOutcome::Returned(val) => provided_layers.push(val),
            other => return classify_bg_failure(heap, other),
        }
    }
    for (handler_name, kind) in &input.handler_globals {
        let reps: Vec<Value> = provided_layers.iter().map(Value::representation).collect();
        let spec = SessionRunSpec {
            entry: handler_name,
            await_task: false,
            root_layer: root_layer.as_ref(),
            provided_layers: &reps,
            handlers: &[],
            fuel: input.fuel,
            host_mode: input.host_mode,
            external_interrupt: Some(Arc::clone(&input.interrupt)),
            synthetic_codes: codes,
        };
        let report = execute_session_step(heap, &input.program, execution, persistent, &spec);
        stdout.push_str(&report.stdout);
        match report.outcome {
            ProgramOutcome::Returned(closure) => handlers.push((closure, *kind)),
            other => return classify_bg_failure(heap, other),
        }
    }
    let provided_reps: Vec<Value> = provided_layers.iter().map(Value::representation).collect();
    let handler_reps: Vec<(Value, SessionHandlerKind)> = handlers
        .iter()
        .map(|(v, k)| (v.representation(), *k))
        .collect();
    let spec = SessionRunSpec {
        entry: &input.entry,
        await_task: true,
        root_layer: root_layer.as_ref(),
        provided_layers: &provided_reps,
        handlers: &handler_reps,
        fuel: input.fuel,
        host_mode: input.host_mode,
        external_interrupt: Some(Arc::clone(&input.interrupt)),
        synthetic_codes: codes,
    };
    let report = execute_session_step(heap, &input.program, execution, persistent, &spec);
    stdout.push_str(&report.stdout);
    match report.outcome {
        ProgramOutcome::Returned(value) => {
            let rendered = render_value(heap, &value).unwrap_or_else(|_| "<value>".to_owned());
            let _ = heap.drop_value(value);
            JobStatus::Done(rendered)
        }
        other => classify_bg_failure(heap, other),
    }
}

fn classify_bg_failure(heap: &mut Heap, outcome: ProgramOutcome) -> JobStatus {
    match outcome {
        ProgramOutcome::Returned(v) => {
            let _ = heap.drop_value(v);
            JobStatus::Done("()".to_owned())
        }
        ProgramOutcome::Raised(v) => {
            let msg = render_value(heap, &v).unwrap_or_else(|_| "<error>".to_owned());
            let _ = heap.drop_value(v);
            JobStatus::Failed(msg)
        }
        ProgramOutcome::Failed(cause) => {
            let status = if is_interrupt_cause(heap, &cause) {
                JobStatus::Killed
            } else if let Some((effect, op)) = extract_unhandled_effect(heap, &cause) {
                JobStatus::Failed(format!("unhandled effect {effect}.{op}"))
            } else {
                JobStatus::Failed(describe_cause(heap, &cause))
            };
            let _ = heap.drop_cause(cause);
            status
        }
        ProgramOutcome::Fault(err) => JobStatus::Failed(err.to_string()),
    }
}
