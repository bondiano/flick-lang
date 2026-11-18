//! The host primitives of `Host.live` on the counted heap, with the values
//! of the evaluator's deterministic host (`crate::native::host`). The text
//! an operation takes is borrowed; the machine releases it.

use flick_core::ir::Lit;
use flick_core::ir::memory::HostPrimitive;

use super::{Error, Heap, Value};
use crate::native::host::{DRAW, MILLIS, MONOTONIC_NANOS, NANOS};

pub(super) fn run(
    heap: &mut Heap,
    primitive: HostPrimitive,
    value: &Value,
    stdout: &mut String,
) -> Result<Value, Error> {
    match primitive {
        // the model observes stdout only
        HostPrimitive::StderrLine => heap.string(value).map(|_| Value::Unit),
        HostPrimitive::StdoutWrite => {
            stdout.push_str(heap.string(value)?);
            Ok(Value::Unit)
        }
        HostPrimitive::Env => {
            heap.string(value)?;
            heap.construct("Prelude.Option.None", vec![])
        }
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
