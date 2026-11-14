//! The interfaces of the built-in modules, saved in the user's cache
//! directory: checking them takes longer than any project, so each binary
//! checks them once for each `std` it is given, and later runs read what it
//! saved.

use std::num::NonZero;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;
use std::{fs, thread};

use flick_core::build::{self, BuiltinModules, BuiltinSource, BuiltinStore};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::Interface;
use flick_core::ir;
use flick_core::types;
use flick_ir::Program;
use serde::{Deserialize, Serialize};

use crate::fetch;
use crate::saved;
use crate::stdlib;
use crate::timings;
use crate::toolchain;
use flick_core::project::store;
use sha2::{Digest, Sha256};

const PREFIX: &str = "builtins-";

#[cfg(test)]
mod tests;

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
    // other binaries' files are stale once this one runs; its files for
    // other implicit packages serve other projects
    saved::evict_others(&dir, PREFIX, &binary);
    let _ = saved::write(&path, &json);
    Ok(())
}

/// How many threads read the saved interfaces at most.
const READERS: usize = 8;

/// The interfaces of a saved file, one JSON document a line: all of them,
/// or nothing when a line does not read or the file holds another number of
/// them than there are built-in modules, as one cut short does. Reading
/// them is most of what a small check takes, so several threads share the
/// lines, which a string of JSON never breaks.
fn saved(bytes: &[u8]) -> Option<Vec<Interface>> {
    let lines: Vec<&[u8]> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() != types::builtin_sources().len() {
        return None;
    }
    let available = thread::available_parallelism().map_or(1, NonZero::get);
    let readers = available.min(READERS).min(lines.len()).max(1);
    let lines = &lines;
    // every `readers`-th line each: the big modules stand together
    let read = move |first: usize| {
        (lines.iter().skip(first).step_by(readers))
            .map(|line| serde_json::from_slice::<Interface>(line).ok())
            .collect::<Option<Vec<_>>>()
    };
    thread::scope(|scope| {
        let parts: Vec<_> = (0..readers)
            .map(|first| {
                let reader = thread::Builder::new().spawn_scoped(scope, move || read(first));
                reader.map_err(|_| first)
            })
            .collect();
        let mut parts = parts
            .into_iter()
            .map(|part| {
                let read = match part {
                    Ok(reader) => reader.join().ok()?,
                    // a thread the system refuses is work this one does
                    Err(first) => read(first),
                };
                Some(read?.into_iter())
            })
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

fn saved_modules(stages_by_class: bool) -> Option<BuiltinModules> {
    let started = Instant::now();
    let bytes = fs::read(modules_path(stages_by_class)?).ok()?;
    // a file of another version of the IR is lowered again and replaced
    let cached: CachedModules = ciborium::from_reader(&bytes[..]).ok()?;
    let bundle = cached.checked()?;
    timings::phase("built-in modules read from their cache", started.elapsed());
    Some(bundle)
}

#[derive(Serialize, Deserialize)]
struct CachedModules {
    program: Program,
    sources: Vec<String>,
}

impl CachedModules {
    fn new(bundle: &BuiltinModules) -> Self {
        Self {
            program: Program {
                ir_version: flick_ir::VERSION.into(),
                compiler: toolchain::COMPILER.into(),
                files: bundle.sources.iter().map(|s| s.name.clone()).collect(),
                modules: bundle.modules.clone(),
                lang: ir::lang_items(),
            },
            sources: bundle.sources.iter().map(|s| s.text.clone()).collect(),
        }
    }

    fn checked(mut self) -> Option<BuiltinModules> {
        if self.program.ir_version != flick_ir::VERSION
            || self.program.compiler != toolchain::COMPILER
            || self.program.files.len() != self.sources.len()
        {
            return None;
        }
        for module in &mut self.program.modules {
            let mut valid = true;
            module.each_span_mut(&mut |span| {
                valid &= self
                    .sources
                    .get(span.file.index() as usize)
                    .is_some_and(|text| span.start <= span.end && text.get(span.range()).is_some());
            });
            if !valid {
                return None;
            }
        }
        Some(BuiltinModules {
            modules: self.program.modules,
            sources: self
                .program
                .files
                .into_iter()
                .zip(self.sources)
                .map(|(name, text)| BuiltinSource { name, text })
                .collect(),
        })
    }
}

/// A failure to save only means the next build lowers them again.
fn save_modules(stages_by_class: bool, bundle: &BuiltinModules) {
    let Some(path) = modules_path(stages_by_class) else {
        return;
    };
    let cached = CachedModules::new(bundle);
    let mut bytes = Vec::new();
    if ciborium::into_writer(&cached, &mut bytes).is_ok() {
        let _ = saved::write(&path, &bytes);
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
