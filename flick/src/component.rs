//! Shared engine of the compiler's component hosts.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Once, OnceLock};
use std::thread;
use std::time::Duration;

use flick_backend::host::runtime::{self, Cancellation};
use flick_core::project::store;

use crate::child;
use wasmtime::Engine;

#[cfg(any(feature = "wasm-plugins", test))]
pub(crate) use runtime::{MemoryLimits, memory_limits, prepare, why};

fn cache_dir_from(override_dir: Option<OsString>, default_dir: Option<PathBuf>) -> Option<PathBuf> {
    override_dir
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| default_dir.map(|dir| dir.join("wasmtime")))
}

pub(crate) fn engine() -> Result<&'static Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let cache = cache_dir_from(env::var_os("FLICK_WASMTIME_CACHE_DIR"), store::cache_dir());
            if let Some(dir) = &cache {
                let _ = fs::create_dir_all(dir);
            }
            runtime::engine(cache.as_deref())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The cancellation of every component call and host tool of the process.
pub(crate) fn cancellation() -> Result<&'static Cancellation, String> {
    static TOKEN: OnceLock<Cancellation> = OnceLock::new();
    let engine = engine()?;
    Ok(TOKEN.get_or_init(|| Cancellation::new(engine)))
}

/// From now on SIGINT and SIGTERM cancel component calls and host tools,
/// which then end without publishing, instead of ending `flick` at once.
pub(crate) fn cancel_on_signals() {
    static WATCHER: Once = Once::new();
    WATCHER.call_once(|| {
        let Ok(token) = cancellation() else { return };
        let signals = child::signals();
        thread::spawn(move || {
            while signals.load(Ordering::Relaxed) == 0 {
                thread::sleep(Duration::from_millis(20));
            }
            token.cancel();
        });
    });
}

#[cfg(test)]
mod tests;
