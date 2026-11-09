//! The interfaces of the built-in modules, saved in the user's cache
//! directory: checking them takes longer than any project, so each binary
//! checks them once for each `std` it is given, and later runs read what it
//! saved.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::{fs, process};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::Interface;
use flick_core::types;

use crate::fetch;
use crate::stdlib;
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
/// of its own.
pub fn prepare_in(stages_by_class: bool) -> Result<(), Diagnostic> {
    let Some((dir, binary, name)) = location(stages_by_class) else {
        return types::try_builtins(stages_by_class)
            .map(drop)
            .map_err(|p| unfit(&p));
    };
    let path = dir.join(&name);
    if let Some(interfaces) = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Interface>>(&bytes).ok())
    {
        types::install_builtins(stages_by_class, interfaces);
        return Ok(());
    }
    let Ok(json) = serde_json::to_vec(types::try_builtins(stages_by_class).map_err(|p| unfit(&p))?)
    else {
        return Ok(());
    };
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
    let name = format!("{binary}{}{mode}.json", &implicit_hash()[..16]);
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
