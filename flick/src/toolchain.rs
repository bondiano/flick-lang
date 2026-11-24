//! The compiler owns its version, runtime ABI version, and binary fingerprint.

use std::time::UNIX_EPOCH;
use std::{env, fs};

pub const COMPILER: &str = env!("CARGO_PKG_VERSION");
#[cfg(feature = "wasm-backends")]
pub const RUNTIME: &str = "0.2.0";

/// This compiler as caches tell binaries apart: its version, and the size
/// and modification time of its executable, which a rebuild without a new
/// version still changes.
pub fn fingerprint() -> Option<String> {
    let exe = fs::metadata(env::current_exe().ok()?).ok()?;
    let modified = exe.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!(
        "{COMPILER}-{:x}-{:x}",
        exe.len(),
        modified.as_nanos()
    ))
}
