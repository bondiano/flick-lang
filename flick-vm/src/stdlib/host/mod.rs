//! Native Rust host services shared across all Flick VM tiers.
//!
//! Supports both [`HostMode::Deterministic`] (for `flick-model` reference
//! runs) and [`HostMode::LiveOs`] (real OS console, clock, randomness,
//! environment, arguments, files, filesystem, and child processes).

mod clock;
mod console;
mod env;
mod error_map;
mod fs;
mod fs_handle;
mod fs_ops;
mod fs_stat;
mod process;
mod process_cmd;
mod random;

use std::collections::{BTreeMap, BTreeSet};
use std::fs as std_fs;
use std::path::PathBuf;
use std::time::Instant;

use flick_core::ir::memory::{HostPrimitive, HostServiceIntrinsic};

use crate::tier0::{Error, Heap, Value};

use super::hooks::VmHooks;
use super::primitives::IsaacRng;
pub use error_map::{format_uncaught_cause, format_uncaught_raised};
use fs_handle::FsFileEntry;
use process::ProcessEntry;

/// The deterministic wall clock, in milliseconds since the epoch: 2025-06-15T17:46:40Z.
pub const MILLIS: f64 = 1_750_000_000_000.0;

/// The deterministic wall clock in nanoseconds, [`MILLIS`] exactly.
pub const NANOS: i64 = 1_750_000_000_000_000_000;

/// The deterministic monotonic clock, in nanoseconds.
pub const MONOTONIC_NANOS: i64 = 0;

/// Every deterministic draw of `Host.Random.next`, in [0, 1).
pub const DRAW: f64 = 0.5;

/// Selects how host services behave during VM execution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostMode {
    /// Deterministic reference-model host: fixed clock/draw, empty env/vars/args.
    #[default]
    Deterministic,
    /// Live OS process environment, clocks, randomness, filesystem, and child processes.
    LiveOs,
}

/// Execution-owned table of live OS resources (open file handles, scoped temp
/// paths, child processes, monotonic clock anchor, and OS-seeded PRNG state).
///
/// Scoped resources are released deterministically by `Scope` finalizers; any
/// resources still present on abnormal termination (such as fuel exhaustion)
/// are cleaned up in [`Drop`].
#[derive(Debug)]
pub struct HostServices {
    next_handle: i64,
    pub(crate) files: BTreeMap<i64, std_fs::File>,
    pub(crate) fs_files: BTreeMap<i64, FsFileEntry>,
    pub(crate) scoped_temps: BTreeSet<PathBuf>,
    pub(crate) processes: BTreeMap<i64, ProcessEntry>,
    pub(crate) monotonic_start: Instant,
    pub(crate) os_rng: Option<IsaacRng>,
}

impl Default for HostServices {
    fn default() -> Self {
        Self::new()
    }
}

impl HostServices {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            files: BTreeMap::new(),
            fs_files: BTreeMap::new(),
            scoped_temps: BTreeSet::new(),
            processes: BTreeMap::new(),
            monotonic_start: Instant::now(),
            os_rng: None,
        }
    }

    pub(crate) fn alloc_handle(&mut self) -> i64 {
        let id = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        id
    }
}

impl Drop for HostServices {
    fn drop(&mut self) {
        self.files.clear();
        self.fs_files.clear();
        for proc in self.processes.values_mut() {
            proc.force_terminate();
        }
        self.processes.clear();
        let temps: Vec<PathBuf> = self.scoped_temps.iter().cloned().collect();
        self.scoped_temps.clear();
        for path in temps {
            let _ = std_fs::remove_dir_all(&path).or_else(|_| std_fs::remove_file(&path));
        }
    }
}

pub(crate) fn run_with_hooks(
    heap: &mut Heap,
    primitive: HostPrimitive,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    match primitive {
        HostPrimitive::StderrLine => console::stderr_line(heap, value, hooks),
        HostPrimitive::StdoutWrite => console::stdout_write(heap, value, hooks),
        HostPrimitive::Env => env::get_env(heap, value, hooks),
        HostPrimitive::Vars => env::list_vars(heap, hooks),
        HostPrimitive::Args => env::list_args(heap, hooks),
        HostPrimitive::CurrentTimeMillis => Ok(clock::current_time_millis(hooks)),
        HostPrimitive::CurrentTimeNanos => clock::current_time_nanos(heap, hooks),
        HostPrimitive::MonotonicTimeNanos => clock::monotonic_time_nanos(heap, hooks),
        HostPrimitive::RandomNext => Ok(Value::Float(random::next(hooks))),
        HostPrimitive::ProcessSpawn => {
            if hooks.host_mode() == HostMode::Deterministic {
                Err(Error::UnsupportedOperation("a child process"))
            } else {
                process::spawn(heap, value, hooks)
            }
        }
    }
}

pub(crate) fn run_service_with_hooks(
    heap: &mut Heap,
    op: HostServiceIntrinsic,
    args: &[Value],
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    use HostServiceIntrinsic as H;
    heap.admit_values(args)?;
    match op {
        H::ProcessMake
        | H::ProcessSetCwd
        | H::ProcessSetEnv
        | H::ProcessPrefix
        | H::ProcessPipeTo
        | H::ProcessSetStdin
        | H::ProcessInheritOutput
        | H::ProcessSpawn
        | H::ProcessClose
        | H::ProcessPid
        | H::ProcessWait
        | H::ProcessIsRunning
        | H::ProcessKill
        | H::ProcessKillWith
        | H::ProcessReadStdout
        | H::ProcessReadStderr
        | H::ProcessOutputString
        | H::ProcessOutputLines => process::run(heap, op, args, hooks),

        H::FileOpen
        | H::FileClose
        | H::FileReadHandle
        | H::FileRead
        | H::FileWrite
        | H::FsOpen
        | H::FsClose
        | H::FsFileStat
        | H::FsSeek
        | H::FsSync
        | H::FsReadAlloc
        | H::FsFileTruncate
        | H::FsWrite
        | H::FsWriteAll => fs_handle::run(heap, op, args, hooks),

        H::FsCopy
        | H::FsCopyFile
        | H::FsGlob
        | H::FsMakeTempDirectory
        | H::FsMakeTempDirectoryScoped
        | H::FsMakeTempFile
        | H::FsMakeTempFileScoped
        | H::FsRemoveTemp
        | H::FsRemoveTempParent => fs_ops::run(heap, op, args, hooks),

        H::FsAccess
        | H::FsChmod
        | H::FsChown
        | H::FsExists
        | H::FsLink
        | H::FsMakeDirectory
        | H::FsReadDirectory
        | H::FsReadFile
        | H::FsReadFileString
        | H::FsReadLink
        | H::FsRealPath
        | H::FsRemove
        | H::FsRename
        | H::FsStat
        | H::FsSymlink
        | H::FsTruncate
        | H::FsUtimes
        | H::FsWriteFile
        | H::FsWriteFileString => fs::run(heap, op, args, hooks),
    }
}
