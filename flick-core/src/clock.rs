//! Durations for logs. wasm32 has no clock, so everything takes zero there.

use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// `f`'s result and how long it took.
#[cfg(not(target_arch = "wasm32"))]
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let result = f();
    (result, started.elapsed())
}

#[cfg(target_arch = "wasm32")]
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    (f(), Duration::ZERO)
}
