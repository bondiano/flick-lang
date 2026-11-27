//! Host wall-clock and monotonic-clock primitives (`Host.Clock`).

#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
use std::sync::OnceLock;
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use flick_core::ir::Lit;

use crate::stdlib::hooks::VmHooks;
use crate::tier0::{Error, Heap, Value};

use super::{HostMode, MILLIS, MONOTONIC_NANOS, NANOS};

#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
static PROCESS_MONOTONIC_START: OnceLock<Instant> = OnceLock::new();

#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
fn process_monotonic_start() -> Instant {
    *PROCESS_MONOTONIC_START.get_or_init(Instant::now)
}

pub(super) fn current_time_millis(hooks: &mut dyn VmHooks) -> Value {
    if let Some(now_ns) = hooks.virtual_now_ns() {
        #[expect(clippy::cast_precision_loss, reason = "virtual ms fits f64")]
        let ms = (now_ns / 1_000_000) as f64;
        return Value::Float(ms);
    }
    match hooks.host_mode() {
        HostMode::Deterministic => Value::Float(MILLIS),
        HostMode::LiveOs => {
            if let Some(ms) = hooks.host_wall_time_millis() {
                return Value::Float(ms);
            }
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            {
                let millis = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                #[expect(clippy::cast_precision_loss, reason = "epoch ms fits IEEE-754 f64")]
                let ms = millis as f64;
                Value::Float(ms)
            }
            #[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
            {
                Value::Float(MILLIS)
            }
        }
    }
}

pub(super) fn current_time_nanos(heap: &mut Heap, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    if let Some(now_ns) = hooks.virtual_now_ns() {
        return heap.literal(&Lit::BigInt(now_ns.to_string()));
    }
    match hooks.host_mode() {
        HostMode::Deterministic => heap.literal(&Lit::BigInt(NANOS.to_string())),
        HostMode::LiveOs => {
            if let Some(nanos) = hooks.host_wall_time_nanos() {
                return heap.literal(&Lit::BigInt(nanos.to_string()));
            }
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            {
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos());
                heap.literal(&Lit::BigInt(nanos.to_string()))
            }
            #[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
            {
                heap.literal(&Lit::BigInt(NANOS.to_string()))
            }
        }
    }
}

pub(super) fn monotonic_time_nanos(
    heap: &mut Heap,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    if let Some(now_ns) = hooks.virtual_now_ns() {
        return heap.literal(&Lit::BigInt(now_ns.to_string()));
    }
    match hooks.host_mode() {
        HostMode::Deterministic => heap.literal(&Lit::BigInt(MONOTONIC_NANOS.to_string())),
        HostMode::LiveOs => {
            if let Some(nanos) = hooks.host_monotonic_nanos() {
                return heap.literal(&Lit::BigInt(nanos.to_string()));
            }
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            {
                let start = hooks
                    .host_services()
                    .map_or_else(process_monotonic_start, |svc| svc.monotonic_start);
                let nanos = start.elapsed().as_nanos();
                heap.literal(&Lit::BigInt(nanos.to_string()))
            }
            #[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
            {
                heap.literal(&Lit::BigInt(MONOTONIC_NANOS.to_string()))
            }
        }
    }
}
