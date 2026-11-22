//! Child-process supervision, bounded pipe draining, and temporary generation
//! management for `run-flick` executions.

use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};

#[cfg(feature = "wasm-backends")]
use flick_backend::host::runtime::Cancellation;
use flick_core::build::{go, runner_file};
use flick_core::project::{Project, cache};

use crate::artifact::resolve_root;
use crate::child;
use crate::sources::FsHost;

/// How long a `run-flick` program or documentation example may run before the
/// host terminates it.
const TIMEOUT: Duration = Duration::from_mins(1);

/// Maximum stderr bytes retained for failure diagnostics.
const STDERR_TAIL: usize = 64 << 10;

pub(crate) enum RunStatus {
    Success,
    Failed(String),
    TimedOut,
    OutputTooLarge,
    Cancelled,
    LaunchError(String),
}

pub(crate) struct RunOutput {
    #[cfg_attr(not(feature = "wasm-backends"), allow(dead_code))]
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: String,
    pub(crate) status: RunStatus,
}

pub(super) fn fresh_entry_name(project: Option<&Project>) -> String {
    let has_custom_main = project.is_some_and(|p| {
        p.config
            .entries
            .iter()
            .any(|e| e.name == "main" && e.runner.is_some())
    });
    if !has_custom_main {
        return "main".to_owned();
    }
    for n in 0usize.. {
        let candidate = if n == 0 {
            "run-flick".to_owned()
        } else {
            format!("run-flick-{n}")
        };
        let clashes = project.is_some_and(|p| {
            p.config
                .entries
                .iter()
                .any(|e| e.name == candidate && e.runner.is_some())
        });
        if !clashes {
            return candidate;
        }
    }
    unreachable!("unbounded suffix range")
}

pub(super) fn native_launch_command(gen_dir: &Path, entry_name: &str, is_go: bool) -> Command {
    if is_go {
        Command::new(gen_dir.join(go::executable(entry_name)))
    } else {
        let mut command = Command::new("node");
        command
            .arg("--enable-source-maps")
            .arg(gen_dir.join(runner_file(entry_name, false)));
        command
    }
}

pub(super) fn supervise(
    mut child: Child,
    stdout_limit: usize,
    #[cfg(feature = "wasm-backends")] cancel: Option<&Cancellation>,
) -> RunOutput {
    let overflowed = Arc::new(AtomicBool::new(false));
    let stdout_overflow = Arc::clone(&overflowed);
    let stdout_handle = {
        let stream = child.stdout.take();
        thread::spawn(move || read_bounded(stream, stdout_limit, &stdout_overflow))
    };
    let stderr_handle = {
        let stream = child.stderr.take();
        thread::spawn(move || read_tail(stream, STDERR_TAIL))
    };

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if overflowed.load(Ordering::Relaxed) {
                    stop_child(&mut child);
                    let _ = stdout_handle.join();
                    let stderr = join_tail(stderr_handle);
                    return RunOutput {
                        stdout: Vec::new(),
                        stderr,
                        status: RunStatus::OutputTooLarge,
                    };
                }
                let cancelled = child::interrupted() || {
                    #[cfg(feature = "wasm-backends")]
                    {
                        cancel.is_some_and(Cancellation::is_cancelled)
                    }
                    #[cfg(not(feature = "wasm-backends"))]
                    {
                        false
                    }
                };
                if cancelled {
                    stop_child(&mut child);
                    let _ = stdout_handle.join();
                    let stderr = join_tail(stderr_handle);
                    return RunOutput {
                        stdout: Vec::new(),
                        stderr,
                        status: RunStatus::Cancelled,
                    };
                }
                if Instant::now() >= deadline {
                    stop_child(&mut child);
                    let _ = stdout_handle.join();
                    let stderr = join_tail(stderr_handle);
                    return RunOutput {
                        stdout: Vec::new(),
                        stderr,
                        status: RunStatus::TimedOut,
                    };
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                stop_child(&mut child);
                let _ = stdout_handle.join();
                let stderr = join_tail(stderr_handle);
                return RunOutput {
                    stdout: Vec::new(),
                    stderr,
                    status: RunStatus::Failed(err.to_string()),
                };
            }
        }
    };

    end_group(&child);
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = join_tail(stderr_handle);

    if overflowed.load(Ordering::Relaxed) {
        return RunOutput {
            stdout: Vec::new(),
            stderr,
            status: RunStatus::OutputTooLarge,
        };
    }

    if status.success() {
        RunOutput {
            stdout,
            stderr,
            status: RunStatus::Success,
        }
    } else {
        RunOutput {
            stdout,
            stderr,
            status: RunStatus::Failed(format!("exited with {status}")),
        }
    }
}

fn read_bounded(stream: Option<ChildStdout>, limit: usize, overflowed: &AtomicBool) -> Vec<u8> {
    let Some(mut stream) = stream else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(n @ 1..) = stream.read(&mut buf) {
        if out.len().saturating_add(n) > limit {
            overflowed.store(true, Ordering::Relaxed);
            return Vec::new();
        }
        out.extend_from_slice(&buf[..n]);
    }
    out
}

fn read_tail(stream: Option<ChildStderr>, max_bytes: usize) -> Vec<u8> {
    let Some(mut stream) = stream else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(n @ 1..) = stream.read(&mut buf) {
        kept.extend_from_slice(&buf[..n]);
        if kept.len() > 2 * max_bytes {
            kept.drain(..kept.len() - max_bytes);
        }
    }
    let start = kept.len().saturating_sub(max_bytes);
    kept.split_off(start)
}

fn join_tail(handle: thread::JoinHandle<Vec<u8>>) -> String {
    let bytes = handle.join().unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn end_group(child: &Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let _ = kill_process_group(Pid::from_child(child), Signal::KILL);
    }
    #[cfg(not(unix))]
    let _ = child;
}

fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        let group = Pid::from_child(child);
        let _ = kill_process_group(group, Signal::TERM);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait().is_ok_and(|s| s.is_some()) {
                let _ = kill_process_group(group, Signal::KILL);
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = kill_process_group(group, Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

static GENERATION_SEQ: AtomicU64 = AtomicU64::new(1);

/// An isolated generation directory cleaned up on drop.
pub(super) struct TempGeneration {
    path: PathBuf,
    run_flick_dir: Option<PathBuf>,
    created_flick_dir: Option<PathBuf>,
}

impl TempGeneration {
    pub(super) fn reserve(host: &FsHost, project: Option<&Project>) -> io::Result<Self> {
        let base_root =
            project.map_or_else(|| host.cwd.clone(), |p| resolve_root(&p.root, &p.root));
        let flick_dir = base_root.join(cache::DIR);
        let flick_existed = flick_dir.exists();
        let run_flick_dir = flick_dir.join("run-flick");
        if fs::create_dir_all(&run_flick_dir).is_ok()
            && let Ok(path) = create_unique_dir(&run_flick_dir)
        {
            return Ok(Self {
                path,
                run_flick_dir: Some(run_flick_dir),
                created_flick_dir: (project.is_none() && !flick_existed).then_some(flick_dir),
            });
        }
        let fallback_root = env::temp_dir().join("flick-run-flick");
        fs::create_dir_all(&fallback_root)?;
        let path = create_unique_dir(&fallback_root)?;
        Ok(Self {
            path,
            run_flick_dir: None,
            created_flick_dir: None,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempGeneration {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        if let Some(dir) = &self.run_flick_dir {
            let _ = fs::remove_dir(dir);
        }
        if let Some(dir) = &self.created_flick_dir {
            let _ = fs::remove_dir(dir);
        }
    }
}

fn create_unique_dir(parent: &Path) -> io::Result<PathBuf> {
    let pid = process::id();
    for _ in 0..1024 {
        let seq = GENERATION_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("run-{pid}-{seq}"));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::other(
        "temporary run-flick generation identifiers exhausted",
    ))
}
