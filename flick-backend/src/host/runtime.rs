//! Wasmtime configuration and resource accounting shared by both hosts.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{thread, time::Duration};
use wasmtime::Error;

use wasmtime::{Cache, CacheConfig, Config, Engine, Store, Trap, UpdateDeadline};

pub fn engine(cache_dir: Option<&Path>) -> Result<Engine, String> {
    let cache = cache_dir.and_then(|dir| {
        let mut config = CacheConfig::new();
        config.with_directory(dir);
        Cache::new(config).ok()
    });
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.cranelift_nan_canonicalization(true);
    config.cache(cache);
    Engine::new(&config).map_err(|e| e.to_string())
}

pub use super::memory::{MemoryLimits, memory_limits};

/// The transfer budget is separate from instruction fuel and is checked
/// while lifting guest data, before allocating the complete Rust result.
pub fn prepare<T>(store: &mut Store<T>, fuel: u64, output: usize) -> wasmtime::Result<()> {
    store.set_fuel(fuel)?;
    store.set_hostcall_fuel(output);
    store.epoch_deadline_callback(|_| Ok(UpdateDeadline::Continue(1)));
    store.set_epoch_deadline(1);
    Ok(())
}

/// One build's cancellation, independent of other stores using the engine.
#[derive(Clone)]
pub struct Cancellation {
    engine: Engine,
    shared: Arc<CancelState>,
}

struct CancelState {
    cancelled: AtomicBool,
    active: AtomicUsize,
}

pub(crate) struct ActiveCall(Arc<CancelState>);

impl Drop for ActiveCall {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Cancellation {
    pub fn new(engine: &Engine) -> Self {
        Self {
            engine: engine.clone(),
            shared: Arc::new(CancelState {
                cancelled: AtomicBool::new(false),
                active: AtomicUsize::new(0),
            }),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.shared.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("component call cancelled".into())
        } else {
            Ok(())
        }
    }

    pub fn cancel(&self) {
        if self.shared.cancelled.swap(true, Ordering::AcqRel) {
            return;
        }
        self.engine.increment_epoch();
        if self.shared.active.load(Ordering::Acquire) != 0 {
            let token = self.clone();
            // Repeat while a call is active, including when another token's
            // epoch callback is concurrently rearming this store.
            drop(thread::spawn(move || {
                while token.shared.active.load(Ordering::Acquire) != 0 {
                    token.engine.increment_epoch();
                    thread::sleep(Duration::from_millis(1));
                }
            }));
        }
    }

    pub(crate) fn enter<T>(&self, store: &mut Store<T>) -> Result<ActiveCall, String> {
        if !Engine::same(&self.engine, store.engine()) {
            return Err("cancellation token belongs to another engine".into());
        }
        self.shared.active.fetch_add(1, Ordering::AcqRel);
        let guard = ActiveCall(self.shared.clone());
        if self.is_cancelled() {
            return Err("component call cancelled".into());
        }
        let state = self.shared.clone();
        store.epoch_deadline_callback(move |_| {
            if state.cancelled.load(Ordering::Acquire) {
                Err(Error::msg("component call cancelled"))
            } else {
                Ok(UpdateDeadline::Continue(1))
            }
        });
        store.set_epoch_deadline(1);
        Ok(guard)
    }
}

pub fn why(error: &wasmtime::Error) -> String {
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => "ran out of its fuel".into(),
        Some(Trap::AllocationTooLarge) => "ran out of its memory".into(),
        _ if format!("{error:?}").contains("component memory budget exceeded") => {
            "ran out of its memory".into()
        }
        _ if format!("{error:?}").contains("component call cancelled") => {
            "component call cancelled".into()
        }
        _ if format!("{error:?}").contains("forcing trap when growing memory") => {
            "ran out of its memory".into()
        }
        _ if format!("{error:?}").contains("fuel allocated for hostcalls has been exhausted") => {
            "exceeded its output limit".into()
        }
        _ => format!("trapped: {error}"),
    }
}
