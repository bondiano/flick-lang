//! The host primitives of `Host.live` on the counted heap, supporting both
//! the deterministic reference host and live OS process environment
//! (`HostMode::LiveOs`) through caller-supplied [`VmHooks`].
//! The text an operation takes is borrowed; the caller releases it.

use std::env;

use flick_core::ir::Lit;
use flick_core::ir::memory::HostPrimitive;

use crate::tier0::{Error, Heap, Value};

use super::hooks::VmHooks;

/// The deterministic wall clock, in milliseconds since the epoch: 2025-06-15T17:46:40Z.
pub const MILLIS: f64 = 1_750_000_000_000.0;

/// The deterministic wall clock in nanoseconds, [`MILLIS`] exactly.
pub const NANOS: i64 = 1_750_000_000_000_000_000;

/// The deterministic monotonic clock, in nanoseconds.
pub const MONOTONIC_NANOS: i64 = 0;

/// Every deterministic draw of `Host.Random.next`, in [0, 1).
pub const DRAW: f64 = 0.5;

/// Selects how host environment primitives (`Platform.env`, `Platform.vars`,
/// `Platform.args`) behave during counted execution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostMode {
    /// Deterministic reference-model host: empty env/vars/args.
    #[default]
    Deterministic,
    /// Live OS process environment (`std::env::var`, `std::env::vars`),
    /// allocated into fresh reference-counted heap objects.
    LiveOs,
}

pub(crate) fn run_with_hooks(
    heap: &mut Heap,
    primitive: HostPrimitive,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    match primitive {
        // the model observes stdout only
        HostPrimitive::StderrLine => heap.string_lossy(value).map(|_| Value::Unit),
        HostPrimitive::StdoutWrite => {
            hooks.write_stdout(&heap.string_lossy(value)?);
            Ok(Value::Unit)
        }
        HostPrimitive::Env => {
            let key = heap.string(value)?.to_owned();
            match hooks.host_mode() {
                HostMode::Deterministic => heap.construct("Prelude.Option.None", vec![]),
                HostMode::LiveOs => match env::var(&key) {
                    Ok(found) => {
                        let text = heap.literal(&Lit::String(found))?;
                        heap.construct("Prelude.Option.Some", vec![text])
                    }
                    Err(_) => heap.construct("Prelude.Option.None", vec![]),
                },
            }
        }
        HostPrimitive::Vars => match hooks.host_mode() {
            HostMode::Deterministic => heap.construct("$list", vec![]),
            HostMode::LiveOs => live_env_vars(heap),
        },
        HostPrimitive::Args => heap.construct("$list", vec![]),
        HostPrimitive::CurrentTimeMillis => {
            if let Some(now_ns) = hooks.virtual_now_ns() {
                #[expect(clippy::cast_precision_loss, reason = "virtual ms fits f64")]
                let ms = (now_ns / 1_000_000) as f64;
                Ok(Value::Float(ms))
            } else {
                Ok(Value::Float(MILLIS))
            }
        }
        HostPrimitive::CurrentTimeNanos => {
            if let Some(now_ns) = hooks.virtual_now_ns() {
                heap.literal(&Lit::BigInt(now_ns.to_string()))
            } else {
                heap.literal(&Lit::BigInt(NANOS.to_string()))
            }
        }
        HostPrimitive::MonotonicTimeNanos => {
            if let Some(now_ns) = hooks.virtual_now_ns() {
                heap.literal(&Lit::BigInt(now_ns.to_string()))
            } else {
                heap.literal(&Lit::BigInt(MONOTONIC_NANOS.to_string()))
            }
        }
        HostPrimitive::RandomNext => Ok(Value::Float(hooks.next_seeded_random().unwrap_or(DRAW))),
        // the model's host has no processes to start
        HostPrimitive::ProcessSpawn => Err(Error::UnsupportedOperation("a child process")),
    }
}

fn live_env_vars(heap: &mut Heap) -> Result<Value, Error> {
    let mut pairs = Vec::new();
    for (k, v) in env::vars() {
        let key = match heap.literal(&Lit::String(k)) {
            Ok(val) => val,
            Err(err) => {
                heap.drop_values(pairs)?;
                return Err(err);
            }
        };
        let val = match heap.literal(&Lit::String(v)) {
            Ok(val) => val,
            Err(err) => {
                heap.drop_value(key)?;
                heap.drop_values(pairs)?;
                return Err(err);
            }
        };
        match heap.construct("$tuple", vec![key, val]) {
            Ok(pair) => pairs.push(pair),
            Err(err) => {
                heap.drop_values(pairs)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", pairs)
}
