//! Tier-neutral standard primitive library, cooperative runtime helpers, and
//! host modes shared across all Flick VM tiers.
//!
//! Callers (such as [`crate::tier0`] or future bytecode/JIT tiers) supply a
//! [`VmHooks`] / [`Accounting`] implementation for fuel metering, allocation
//! observation, stdout capture, virtual clocks, and seeded randomness. Values
//! enter borrowed and exit owned on the caller's [`Heap`], preserving exact
//! reference-count and allocation accounting across every tier.

use flick_core::ir::BinOp;
use flick_core::ir::memory::{HostPrimitive, HostServiceIntrinsic, Intrinsic};
use flick_core::numeric::Num;

use crate::tier0::{Error, Heap, Value};

pub(crate) mod cli;
mod hooks;
mod host;
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub(crate) mod host_bridge;
mod host_callbacks;
pub(crate) mod primitives;
pub(crate) mod runtime;

pub use hooks::{Accounting, CountingHooks, FuelCounter, HostContext, HostEnv, Unmetered, VmHooks};
pub(crate) use hooks::{Fuel, charge, with_alloc_accounting};
pub use host::{
    DRAW, HostMode, HostServices, MILLIS, MONOTONIC_NANOS, NANOS, format_uncaught_cause,
    format_uncaught_raised,
};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub use host_bridge::HostBridge;
pub use host_callbacks::{HostCallbacks, HostFetchRequest, HostFetchResponse};
pub use primitives::{is_metered_primitive, parse_float, parse_number, show_value};
pub use runtime::{DEFAULT_VIRTUAL_NOW_NS, FIBER_QUANTUM, QUANTUM_VIRTUAL_NS};

/// Evaluates a binary IR operation (`+`, `-`, `*`, `/`, `%`, `**`, `==`, `!=`,
/// `===`, `<`, `<=`, `>`, `>=`, `&&`, `||`, `++`, list `++`) with caller-supplied
/// fuel and allocation accounting.
pub fn eval_binary(
    heap: &mut Heap,
    op: BinOp,
    left: &Value,
    right: &Value,
    hooks: &mut dyn Accounting,
) -> Result<Value, Error> {
    with_alloc_accounting(heap, hooks, |heap, hooks| {
        primitives::binary_metered(heap, op, left, right, hooks)
    })
}

/// Evaluates a binary IR operation from a two-element operand slice.
pub fn eval_binary_slice(
    heap: &mut Heap,
    op: BinOp,
    args: &[Value],
    hooks: &mut dyn Accounting,
) -> Result<Value, Error> {
    heap.admit_values(args)?;
    eval_binary(heap, op, &args[0], &args[1], hooks)
}

/// Evaluates boolean negation (`not`).
pub fn eval_not(value: &Value) -> Result<Value, Error> {
    primitives::not(value)
}

/// Evaluates numeric unary negation (`-`) for all numeric types including `BigInt`.
pub fn eval_neg(
    heap: &mut Heap,
    num: Num,
    value: &Value,
    hooks: &mut dyn Accounting,
) -> Result<Value, Error> {
    with_alloc_accounting(heap, hooks, |heap, _| primitives::neg(heap, num, value))
}

/// Evaluates `String.starts_with` / `String.includes` predicates.
pub fn eval_string_predicate(
    heap: &Heap,
    kind: Intrinsic,
    needle: &Value,
    subject: &Value,
) -> Result<Value, Error> {
    primitives::string_predicate(heap, kind, needle, subject)
}

/// Evaluates any pure/metered standard-library primitive intrinsic with
/// caller-supplied fuel and allocation accounting.
pub fn eval_primitive(
    heap: &mut Heap,
    kind: Intrinsic,
    arguments: &[Value],
    hooks: &mut dyn Accounting,
) -> Result<Value, Error> {
    with_alloc_accounting(heap, hooks, |heap, hooks| {
        primitives::metered_primitive(heap, kind, arguments, hooks)
    })
}

/// Evaluates `PrintLine`, `Show`, or `Host(_)` intrinsics through the caller's
/// [`VmHooks`] environment and accounting hooks.
pub fn eval_intrinsic(
    heap: &mut Heap,
    kind: Intrinsic,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let before = heap.stats();
    let result = primitives::intrinsic_with_hooks(heap, kind, value, hooks);
    let after = heap.stats();
    let alloc_delta = after.allocations.saturating_sub(before.allocations);
    let free_delta = after.frees.saturating_sub(before.frees);
    if alloc_delta > 0 || free_delta > 0 {
        hooks.record_allocation_delta(alloc_delta, free_delta);
    }
    result
}

/// Evaluates a Stage 5 host primitive (`HostPrimitive`) using the caller's
/// [`VmHooks`] (mode, stdout/stderr sinks, CLI args, optional virtual clock,
/// seeded RNG, and live host services).
pub fn eval_host_primitive(
    heap: &mut Heap,
    primitive: HostPrimitive,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    eval_intrinsic(heap, Intrinsic::Host(primitive), value, hooks)
}

/// Evaluates a native Rust VM host service intrinsic (`HostServiceIntrinsic`)
/// using the caller's [`VmHooks`]. Shared across all VM tiers.
pub fn eval_host_service(
    heap: &mut Heap,
    op: HostServiceIntrinsic,
    args: &[Value],
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let before = heap.stats();
    let result = host::run_service_with_hooks(heap, op, args, hooks);
    let after = heap.stats();
    let alloc_delta = after.allocations.saturating_sub(before.allocations);
    let free_delta = after.frees.saturating_sub(before.frees);
    if alloc_delta > 0 || free_delta > 0 {
        hooks.record_allocation_delta(alloc_delta, free_delta);
    }
    result
}

/// Structural value equality (`==`) with caller-supplied fuel accounting.
pub fn values_equal(
    heap: &Heap,
    left: &Value,
    right: &Value,
    hooks: &mut dyn Accounting,
) -> Result<bool, Error> {
    primitives::values_equal(heap, left, right, hooks)
}

#[cfg(test)]
mod hooks_tests {
    use std::slice;

    use flick_core::ir::Lit;
    use flick_core::ir::memory::{HostPrimitive, IntIntrinsic, Intrinsic};

    use super::*;

    #[test]
    fn counting_hooks_observe_exact_fuel_and_allocations() {
        let mut heap = Heap::new();
        let mut fuel = 100;
        let mut hooks = CountingHooks::new(&mut fuel);
        let text = heap
            .literal(&Lit::String("42".into()))
            .expect("string literal");
        let before_allocs = heap.stats().allocations;

        let res = eval_primitive(
            &mut heap,
            Intrinsic::Int(IntIntrinsic::Parse),
            slice::from_ref(&text),
            &mut hooks,
        )
        .expect("parse int");

        let delta = heap.stats().allocations - before_allocs;
        assert_eq!(hooks.allocations_observed, delta);
        assert_eq!(hooks.fuel_charged, 3); // 1 base + 2 chars
        assert_eq!(fuel, 97);

        heap.drop_values(vec![text, res]).expect("drop values");
        heap.finish().expect("zero leaks");
    }

    #[test]
    fn host_context_routes_stdout_virtual_clock_and_rng() {
        let mut heap = Heap::new();
        let mut fuel = 10;
        let mut stdout = String::new();
        let mut rng = primitives::IsaacRng::from_seed(7.0);
        let expected_draw = primitives::IsaacRng::from_seed(7.0).next_f64();
        let mut ctx = HostContext::new(&mut fuel, HostMode::Deterministic, &mut stdout)
            .with_virtual_now_ns(12_345_000_000)
            .with_seeded_rng(&mut rng);

        let msg = heap.literal(&Lit::String("hi".into())).expect("msg");
        let unit =
            eval_intrinsic(&mut heap, Intrinsic::PrintLine, &msg, &mut ctx).expect("print line");
        assert_eq!(unit, Value::Unit);

        let ms = eval_host_primitive(
            &mut heap,
            HostPrimitive::CurrentTimeMillis,
            &Value::Unit,
            &mut ctx,
        )
        .expect("millis");
        assert_eq!(ms, Value::Float(12_345.0));

        let draw =
            eval_host_primitive(&mut heap, HostPrimitive::RandomNext, &Value::Unit, &mut ctx)
                .expect("random draw");
        assert_eq!(draw, Value::Float(expected_draw));
        assert_eq!(stdout, "hi\n");

        heap.drop_value(msg).expect("drop msg");
        heap.finish().expect("zero leaks");
    }
}
