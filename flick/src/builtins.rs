//! The interfaces of the built-in modules, saved in the user's cache
//! directory: checking them takes longer than any project, so each binary
//! checks them once for each `std` it is given, and later runs read what it
//! saved.

use std::num::NonZero;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;
use std::{fs, process, thread};

use flick_core::build::{self, BuiltinStore};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::Interface;
use flick_core::types;
use flick_ir::{Module, Program};

use crate::fetch;
use crate::stdlib;
use crate::timings;
use crate::toolchain;
use flick_core::project::store;
use sha2::{Digest, Sha256};

const PREFIX: &str = "builtins-";

/// Installs the saved interfaces, or checks the built-in modules and saves
/// them. A failure to read or save only means they are checked again; an
/// implicit package that does not check with this compiler is `FLICK-P043`.
pub fn prepare() -> Result<(), Diagnostic> {
    prepare_in(false)
}

/// [`prepare`], for checks with `stages_by_class` or without, each with a file
/// of its own. Once a process for each: a watching command, the passes of
/// `--fix` and a build inside `run` ask again.
pub fn prepare_in(stages_by_class: bool) -> Result<(), Diagnostic> {
    static PREPARED: [OnceLock<Result<(), Diagnostic>>; 2] = [OnceLock::new(), OnceLock::new()];
    PREPARED[usize::from(stages_by_class)]
        .get_or_init(|| read_or_check(stages_by_class))
        .clone()
}

fn read_or_check(stages_by_class: bool) -> Result<(), Diagnostic> {
    let Some((dir, binary, name)) = location(stages_by_class) else {
        return types::try_builtins(stages_by_class)
            .map(drop)
            .map_err(|p| unfit(&p));
    };
    let path = dir.join(&name);
    let started = Instant::now();
    if let Some(interfaces) = fs::read(&path).ok().and_then(|bytes| saved(&bytes)) {
        timings::phase(
            "built-in interfaces read from their cache",
            started.elapsed(),
        );
        types::install_builtins(stages_by_class, interfaces);
        return Ok(());
    }
    let checked = types::try_builtins(stages_by_class).map_err(|p| unfit(&p))?;
    timings::phase("built-in modules checked", started.elapsed());
    let mut json = Vec::new();
    for interface in checked {
        if serde_json::to_writer(&mut json, interface).is_err() {
            return Ok(());
        }
        json.push(b'\n');
    }
    if fs::create_dir_all(&dir).is_err() {
        return Ok(());
    }
    // other binaries' files are stale once this one runs; its files for
    // other implicit packages serve other projects
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file.starts_with(PREFIX) && !file.starts_with(&binary) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let pending = dir.join(format!("{name}.{}.tmp", process::id()));
    if fs::write(&pending, json).is_ok() && fs::rename(&pending, &path).is_err() {
        let _ = fs::remove_file(&pending);
    }
    Ok(())
}

/// How many threads read the saved interfaces at most.
const READERS: usize = 8;

/// The interfaces of a saved file, one JSON document a line. Reading them
/// is most of what a small check takes, so several threads share the lines,
/// which a string of JSON never breaks.
fn saved(bytes: &[u8]) -> Option<Vec<Interface>> {
    let lines: Vec<&[u8]> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    let available = thread::available_parallelism().map_or(1, NonZero::get);
    let readers = available.min(READERS).min(lines.len()).max(1);
    let lines = &lines;
    thread::scope(|scope| {
        // every `readers`-th line each: the big modules stand together
        let parts: Vec<_> = (0..readers)
            .map(|first| {
                scope.spawn(move || {
                    (lines.iter().skip(first).step_by(readers))
                        .map(|line| serde_json::from_slice::<Interface>(line).ok())
                        .collect::<Option<Vec<_>>>()
                })
            })
            .collect();
        let mut parts = parts
            .into_iter()
            .map(|part| Some(part.join().ok()??.into_iter()))
            .collect::<Option<Vec<_>>>()?;
        (0..lines.len())
            .map(|line| parts[line % readers].next())
            .collect()
    })
}

/// Keeps the lowered built-in modules beside their interfaces, as a program
/// in the IR's CBOR: only the first build of a compiler with a `std` checks
/// and lowers them, and a command that builds nothing reads none of it.
pub fn keep_modules() {
    build::keep_builtin_modules(BuiltinStore {
        load: saved_modules,
        save: save_modules,
    });
}

fn modules_path(stages_by_class: bool) -> Option<PathBuf> {
    let (dir, _, name) = location(stages_by_class)?;
    Some(dir.join(name).with_extension("ir.cbor"))
}

fn saved_modules(stages_by_class: bool) -> Option<Vec<Module>> {
    let started = Instant::now();
    let bytes = fs::read(modules_path(stages_by_class)?).ok()?;
    // a file of another version of the IR is lowered again and replaced
    let program = Program::from_cbor(&bytes).ok()?;
    timings::phase("built-in modules read from their cache", started.elapsed());
    Some(program.modules)
}

/// A failure to save only means the next build lowers them again.
fn save_modules(stages_by_class: bool, modules: &[Module]) {
    let Some(path) = modules_path(stages_by_class) else {
        return;
    };
    // every built-in module is the one file of its own source map
    let program = Program::new(toolchain::COMPILER, modules.to_vec(), &|_| String::new());
    let pending = path.with_extension(format!("cbor.{}.tmp", process::id()));
    if path
        .parent()
        .is_some_and(|dir| fs::create_dir_all(dir).is_ok())
        && fs::write(&pending, program.to_cbor()).is_ok()
        && fs::rename(&pending, &path).is_err()
    {
        let _ = fs::remove_file(&pending);
    }
}

/// What `problem`, the first error of an implicit module, means for the run.
fn unfit(problem: &str) -> Diagnostic {
    let from = stdlib::std_dir().map_or_else(String::new, |d| format!(" at `{}`", d.display()));
    let message = format!(
        "the `std`{from} does not fit flick {}: {problem}",
        toolchain::COMPILER
    );
    Diagnostic::new(codes::P043, Severity::Error, message)
}

/// The cache directory, the prefix of this binary's files and the file
/// name for its implicit packages: the interfaces depend on the checker as
/// much as on the sources, so the name follows the executable, not only the
/// version, and the hash of the sources of those packages, which a project picks.
fn location(stages_by_class: bool) -> Option<(PathBuf, String, String)> {
    let dir = store::cache_dir()?;
    let binary = format!("{PREFIX}{}-", toolchain::fingerprint()?);
    let mode = if stages_by_class { "-by-class" } else { "" };
    let name = format!("{binary}{}{mode}.jsonl", &implicit_hash()[..16]);
    Some((dir, binary, name))
}

/// The hash of the names and texts of the built-in modules of every implicit
/// package and of the tables that bind them on go: what a cache of anything
/// checked or emitted against them keys on.
pub fn implicit_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hasher = Sha256::new();
        let modules = types::builtin_sources().iter();
        for (name, text) in modules.chain(types::implicit_go_bindings()) {
            hasher.update(name.as_bytes());
            hasher.update([0]);
            hasher.update(text.as_bytes());
            hasher.update([0]);
        }
        fetch::hex(&hasher.finalize())
    })
}
