//! The interfaces of the built-in modules, saved in the user's cache
//! directory: checking them takes longer than any project, so each binary
//! checks them once for each `std` it is given, and later runs read what it
//! saved.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;
use std::{env, fs, process};

use flick_core::iface::Interface;
use flick_core::types;

use crate::fetch;
use crate::toolchain;
use flick_core::project::store;
use sha2::{Digest, Sha256};

const PREFIX: &str = "builtins-";

/// Installs the saved interfaces, or checks the built-in modules and saves
/// them. Any failure only means they are checked again.
pub fn prepare() {
    let Some((dir, binary, name)) = location() else {
        return;
    };
    let path = dir.join(&name);
    if let Some(interfaces) = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Interface>>(&bytes).ok())
    {
        types::install_builtins(interfaces);
        return;
    }
    let Ok(json) = serde_json::to_vec(types::builtins()) else {
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    // other binaries' files are stale once this one runs; its files for
    // other `std`s serve other projects
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
}

/// The cache directory, the prefix of this binary's files and the file
/// name for its `std`: the interfaces depend on the checker as much as on
/// the sources, so the name follows the executable, not only the version,
/// and the hash of the sources of `std`, which a project picks.
fn location() -> Option<(PathBuf, String, String)> {
    let dir = store::cache_dir()?;
    let exe = fs::metadata(env::current_exe().ok()?).ok()?;
    let modified = exe.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let binary = format!(
        "{PREFIX}{}-{:x}-{:x}-",
        toolchain::COMPILER,
        exe.len(),
        modified.as_nanos()
    );
    let name = format!("{binary}{}.json", &std_hash()[..16]);
    Some((dir, binary, name))
}

/// The hash of the names and texts of the built-in modules: what a cache of
/// anything checked against `std` keys on.
pub fn std_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hasher = Sha256::new();
        for (module, text) in types::builtin_sources() {
            hasher.update(module.as_bytes());
            hasher.update([0]);
            hasher.update(text.as_bytes());
            hasher.update([0]);
        }
        fetch::hex(&hasher.finalize())
    })
}
