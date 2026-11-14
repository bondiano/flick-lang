//! Shared engine of the compiler's component hosts.

use std::sync::OnceLock;

use flick_backend::host::runtime;
use flick_core::project::store;
use wasmtime::Engine;

pub(crate) use runtime::{MemoryLimits, memory_limits, prepare, why};

pub(crate) fn engine() -> Result<&'static Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let cache = store::cache_dir().map(|dir| dir.join("wasmtime"));
            runtime::engine(cache.as_deref())
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests;
