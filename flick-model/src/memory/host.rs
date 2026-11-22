//! The host primitives of `Host.live` on the counted heap, supporting both
//! the deterministic model host (`crate::native::host`) and live OS process
//! environment (`HostMode::LiveOs`) for live corpus verification. The text
//! an operation takes is borrowed; the machine releases it.

use std::env;

use flick_core::ir::Lit;
use flick_core::ir::memory::HostPrimitive;

use super::{Error, Heap, Value};
use crate::native::host::{DRAW, MILLIS, MONOTONIC_NANOS, NANOS};

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

pub(super) fn run_with_mode(
    heap: &mut Heap,
    mode: HostMode,
    primitive: HostPrimitive,
    value: &Value,
    stdout: &mut String,
) -> Result<Value, Error> {
    match primitive {
        // the model observes stdout only
        HostPrimitive::StderrLine => heap.string_lossy(value).map(|_| Value::Unit),
        HostPrimitive::StdoutWrite => {
            stdout.push_str(&heap.string_lossy(value)?);
            Ok(Value::Unit)
        }
        HostPrimitive::Env => {
            let key = heap.string(value)?.to_owned();
            match mode {
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
        HostPrimitive::Vars => match mode {
            HostMode::Deterministic => heap.construct("$list", vec![]),
            HostMode::LiveOs => live_env_vars(heap),
        },
        HostPrimitive::Args => heap.construct("$list", vec![]),
        HostPrimitive::CurrentTimeMillis => Ok(Value::Float(MILLIS)),
        HostPrimitive::CurrentTimeNanos => heap.literal(&Lit::BigInt(NANOS.to_string())),
        HostPrimitive::MonotonicTimeNanos => {
            heap.literal(&Lit::BigInt(MONOTONIC_NANOS.to_string()))
        }
        HostPrimitive::RandomNext => Ok(Value::Float(DRAW)),
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
