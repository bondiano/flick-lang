//! The interfaces of the built-in modules, saved in the user's cache
//! directory: checking them takes longer than any project, so each binary
//! checks them once and later runs read what it saved.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use std::{env, fs, process};

use flick_core::iface::Interface;
use flick_core::types;

use crate::toolchain;

const PREFIX: &str = "builtins-";

/// Installs the saved interfaces, or checks the built-in modules and saves
/// them. Any failure only means they are checked again.
pub fn prepare() {
    let Some((dir, name)) = location() else {
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
    // other binaries' files are stale once this one runs
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file.starts_with(PREFIX) && *file != *name {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let pending = dir.join(format!("{name}.{}.tmp", process::id()));
    if fs::write(&pending, json).is_ok() && fs::rename(&pending, &path).is_err() {
        let _ = fs::remove_file(&pending);
    }
}

/// The cache directory and the file name for this binary: the interfaces
/// depend on the checker as much as on the sources, so the name follows
/// the executable, not only the version.
fn location() -> Option<(PathBuf, String)> {
    let dir = env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("flick");
    let exe = fs::metadata(env::current_exe().ok()?).ok()?;
    let modified = exe.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let name = format!(
        "{PREFIX}{}-{:x}-{:x}.json",
        toolchain::COMPILER,
        exe.len(),
        modified.as_nanos()
    );
    Some((dir, name))
}
