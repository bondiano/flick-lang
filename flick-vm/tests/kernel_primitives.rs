//! Direct primitive kernel conformance tests against `flick_vm::stdlib`'s
//! tier-neutral interface and caller-supplied fuel/allocation hooks.
//!
//! Exercises the numeric, float formatting, string UTF-16/UTF-8, and record-row
//! boundaries of `docs/backends/numeric-policy.md` and `reference/kernel.md`
//! without going through `tier0::execute_program`, proving that any VM tier can
//! call `flick_vm::stdlib` directly with its own `Accounting` / `VmHooks`.

use std::slice;

use flick_core::ir::memory::{
    BytesIntrinsic, FixedWidthIntrinsic, FloatIntrinsic, IntIntrinsic, Intrinsic, StringIntrinsic,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_vm::stdlib::{
    CountingHooks, FuelCounter, HostContext, HostMode, eval_binary, eval_intrinsic, eval_neg,
    eval_not, eval_primitive, eval_string_predicate,
};
use flick_vm::tier0::{Error, Heap, Value};

fn eval_bin(
    heap: &mut Heap,
    op: BinOp,
    a: &Value,
    b: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    eval_binary(heap, op, a, b, &mut FuelCounter(fuel))
}

fn eval_prim(
    heap: &mut Heap,
    kind: Intrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    eval_primitive(heap, kind, args, &mut FuelCounter(fuel))
}

fn drop_all(heap: &mut Heap, values: impl IntoIterator<Item = Value>) {
    for value in values {
        heap.drop_value(value).expect("drop value");
    }
}

fn show_str(heap: &mut Heap, value: &Value) -> String {
    let mut fuel = 1_000;
    let mut stdout = String::new();
    let mut hooks = HostContext::new(&mut fuel, HostMode::Deterministic, &mut stdout);
    let shown = eval_intrinsic(heap, Intrinsic::Show, value, &mut hooks).expect("Show");
    let text = heap.string(&shown).expect("Show string").to_owned();
    heap.drop_value(shown).expect("drop shown");
    text
}

#[test]
fn integer_overflow_and_division_by_zero_boundaries() {
    let mut heap = Heap::new();
    let mut fuel = 10_000;

    // Signed wrapping modulo 2^W
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Add, Num::Int),
            &Value::Int(i32::MAX),
            &Value::Int(1),
            &mut fuel
        ),
        Ok(Value::Int(i32::MIN))
    );
    assert_eq!(
        eval_neg(
            &mut heap,
            Num::Int,
            &Value::Int(i32::MIN),
            &mut FuelCounter(&mut fuel)
        ),
        Ok(Value::Int(i32::MIN))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Add, Num::Int8),
            &Value::Int8(127),
            &Value::Int8(1),
            &mut fuel
        ),
        Ok(Value::Int8(-128))
    );
    assert_eq!(
        eval_neg(
            &mut heap,
            Num::Int8,
            &Value::Int8(-128),
            &mut FuelCounter(&mut fuel)
        ),
        Ok(Value::Int8(-128))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Add, Num::UInt8),
            &Value::UInt8(255),
            &Value::UInt8(1),
            &mut fuel
        ),
        Ok(Value::UInt8(0))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Sub, Num::UInt8),
            &Value::UInt8(0),
            &Value::UInt8(1),
            &mut fuel
        ),
        Ok(Value::UInt8(255))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Mul, Num::UInt32),
            &Value::UInt32(u32::MAX),
            &Value::UInt32(u32::MAX),
            &mut fuel
        ),
        Ok(Value::UInt32(1))
    );

    // Bit shifts mask modulo bit width
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::Int(IntIntrinsic::ShiftLeft),
            &[Value::Int(1), Value::Int(-1)],
            &mut fuel
        ),
        Ok(Value::Int(i32::MIN))
    );
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64ShiftRightUnsigned),
            &[Value::Int64(i64::MIN), Value::Int(63)],
            &mut fuel
        ),
        Ok(Value::Int64(1))
    );
    heap.finish().expect("zero heap leaks");
}

#[test]
fn division_by_zero_and_bigint_boundaries() {
    let mut heap = Heap::new();
    let mut fuel = 10_000;

    // Safe integer division/remainder by zero returns 0; signed min / -1 = min, min % -1 = 0
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Div, Num::Int),
            &Value::Int(5),
            &Value::Int(0),
            &mut fuel
        ),
        Ok(Value::Int(0))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Rem, Num::Int),
            &Value::Int(5),
            &Value::Int(0),
            &mut fuel
        ),
        Ok(Value::Int(0))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Div, Num::Int),
            &Value::Int(i32::MIN),
            &Value::Int(-1),
            &mut fuel
        ),
        Ok(Value::Int(i32::MIN))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Rem, Num::Int),
            &Value::Int(i32::MIN),
            &Value::Int(-1),
            &mut fuel
        ),
        Ok(Value::Int(0))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Div, Num::Int64),
            &Value::Int64(i64::MIN),
            &Value::Int64(-1),
            &mut fuel
        ),
        Ok(Value::Int64(i64::MIN))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Arith(Arith::Div, Num::UInt64),
            &Value::UInt64(u64::MAX),
            &Value::UInt64(0),
            &mut fuel
        ),
        Ok(Value::UInt64(0))
    );

    // Unsafe integer division/remainder by zero returns structured defect errors
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::IntUnsafeDiv,
            &[Value::Int(5), Value::Int(0)],
            &mut fuel
        ),
        Err(Error::IntDivisionByZero { remainder: false })
    );
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::IntUnsafeRem,
            &[Value::Int(5), Value::Int(0)],
            &mut fuel
        ),
        Err(Error::IntDivisionByZero { remainder: true })
    );
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::IntUnsafeDiv,
            &[Value::Int(i32::MIN), Value::Int(-1)],
            &mut fuel
        ),
        Ok(Value::Int(i32::MIN))
    );

    heap.finish().expect("zero heap leaks");
}

#[test]
fn bigint_and_hook_accounting_boundaries() {
    let mut heap = Heap::new();
    let mut fuel = 10_000;

    // BigInt division by zero -> 0n; negative exponent -> BigIntNegativeExponent
    let two = heap
        .literal(&Lit::BigInt("2".into()))
        .expect("kernel test value");
    let zero = heap
        .literal(&Lit::BigInt("0".into()))
        .expect("kernel test value");
    let neg_one = heap
        .literal(&Lit::BigInt("-1".into()))
        .expect("kernel test value");
    let mut hooks = CountingHooks::new(&mut fuel);
    let q = eval_binary(
        &mut heap,
        BinOp::Arith(Arith::Div, Num::BigInt),
        &two,
        &zero,
        &mut hooks,
    )
    .expect("kernel test value");
    assert_eq!(heap.bigint(&q), Ok("0"));
    assert!(hooks.fuel_charged > 0);
    assert_eq!(hooks.allocations_observed, 1);
    assert_eq!(
        eval_binary(
            &mut heap,
            BinOp::Arith(Arith::Pow, Num::BigInt),
            &two,
            &neg_one,
            &mut hooks
        ),
        Err(Error::BigIntNegativeExponent)
    );
    assert_eq!(eval_not(&Value::Bool(true)), Ok(Value::Bool(false)));

    drop_all(&mut heap, [two, zero, neg_one, q]);
    heap.finish().expect("zero heap leaks");
}

#[test]
fn float_formatting_equality_and_ordering_boundaries() {
    let mut heap = Heap::new();
    let mut fuel = 10_000;

    // JS round-trip formatting thresholds
    for (val, expected) in [
        (Value::Float(-0.0), "-0"),
        (Value::Float(f64::NAN), "NaN"),
        (Value::Float(f64::INFINITY), "Infinity"),
        (Value::Float(f64::NEG_INFINITY), "-Infinity"),
        (Value::Float(1e20), "100000000000000000000"),
        (Value::Float(1e21), "1e+21"),
        (Value::Float(0.000_001), "0.000001"),
        (Value::Float(1e-7), "1e-7"),
        (Value::Float(5e-324), "5e-324"),
        (
            Value::Float(1.797_693_134_862_315_7e308),
            "1.7976931348623157e+308",
        ),
        (Value::Int64(i64::MIN), "-9223372036854775808"),
        (Value::UInt64(u64::MAX), "18446744073709551615"),
    ] {
        assert_eq!(show_str(&mut heap, &val), expected);
    }

    // SameValueZero equality and total ordering (NaN last)
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Eq,
            &Value::Float(f64::NAN),
            &Value::Float(f64::NAN),
            &mut fuel
        ),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        eval_bin(
            &mut heap,
            BinOp::Eq,
            &Value::Float(-0.0),
            &Value::Float(0.0),
            &mut fuel
        ),
        Ok(Value::Bool(true))
    );
    let cmp_nan_inf = eval_prim(
        &mut heap,
        Intrinsic::PrimitiveCompare,
        &[Value::Float(f64::NAN), Value::Float(f64::INFINITY)],
        &mut fuel,
    )
    .expect("kernel test value");
    assert_eq!(heap.tag(&cmp_nan_inf), Ok("Prelude.Ordering.GT"));

    // Float.floor(-0.0) preserves -0.0; Float.sqrt(-1.0) is NaN
    let neg_zero_floor = eval_prim(
        &mut heap,
        Intrinsic::Float(FloatIntrinsic::Floor),
        &[Value::Float(-0.0)],
        &mut fuel,
    )
    .expect("kernel test value");
    assert_eq!(show_str(&mut heap, &neg_zero_floor), "-0");

    let sqrt_neg = eval_prim(
        &mut heap,
        Intrinsic::Float(FloatIntrinsic::Sqrt),
        &[Value::Float(-1.0)],
        &mut fuel,
    )
    .expect("kernel test value");
    assert_eq!(show_str(&mut heap, &sqrt_neg), "NaN");

    // Math.log has the bits JS gives, and next_pow2 is exact at 2^29
    let log_ten = eval_prim(
        &mut heap,
        Intrinsic::Float(FloatIntrinsic::Log),
        &[Value::Float(10.0)],
        &mut fuel,
    )
    .expect("kernel test value");
    assert_eq!(show_str(&mut heap, &log_ten), "2.302585092994046");
    let pow = eval_prim(
        &mut heap,
        Intrinsic::Float(FloatIntrinsic::NextPow2),
        &[Value::Float(536_870_912.0)],
        &mut fuel,
    )
    .expect("kernel test value");
    assert_eq!(show_str(&mut heap, &pow), "536870912");

    heap.drop_value(cmp_nan_inf).expect("kernel test value");
    heap.finish().expect("zero heap leaks");
}

#[test]
fn utf16_string_indexing_lone_surrogates_and_utf8_boundaries() {
    let mut heap = Heap::new();
    let mut fuel = 10_000;

    let earth = heap
        .literal(&Lit::String("a\u{1F30D}b".into()))
        .expect("kernel test value");
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::StringLength,
            slice::from_ref(&earth),
            &mut fuel
        ),
        Ok(Value::Int(4))
    );

    // Slice splits supplementary scalar into lone surrogates and rejoins back
    let slice_args_1 = [
        Value::Int(1),
        Value::Int(2),
        heap.dup(&earth).expect("kernel test value"),
    ];
    let high = eval_prim(
        &mut heap,
        Intrinsic::String(StringIntrinsic::Slice),
        &slice_args_1,
        &mut fuel,
    )
    .expect("kernel test value");
    drop_all(&mut heap, slice_args_1);

    let slice_args_2 = [
        Value::Int(2),
        Value::Int(3),
        heap.dup(&earth).expect("kernel test value"),
    ];
    let low = eval_prim(
        &mut heap,
        Intrinsic::String(StringIntrinsic::Slice),
        &slice_args_2,
        &mut fuel,
    )
    .expect("kernel test value");
    drop_all(&mut heap, slice_args_2);
    assert_eq!(show_str(&mut heap, &high), "\"\\ud83c\"");
    assert_eq!(
        eval_prim(
            &mut heap,
            Intrinsic::StringLength,
            slice::from_ref(&high),
            &mut fuel
        ),
        Ok(Value::Int(1))
    );

    let rejoined =
        eval_bin(&mut heap, BinOp::Concat, &high, &low, &mut fuel).expect("kernel test value");
    let globe = heap
        .literal(&Lit::String("\u{1F30D}".into()))
        .expect("kernel test value");
    assert_eq!(
        eval_bin(&mut heap, BinOp::Eq, &rejoined, &globe, &mut fuel),
        Ok(Value::Bool(true))
    );

    // Predicate and index_of on lone surrogate
    assert_eq!(
        eval_string_predicate(&heap, Intrinsic::StringIncludes, &low, &earth),
        Ok(Value::Bool(true))
    );
    let idx_args = [
        heap.dup(&low).expect("kernel test value"),
        heap.dup(&earth).expect("kernel test value"),
    ];
    let idx = eval_prim(
        &mut heap,
        Intrinsic::String(StringIntrinsic::IndexOf),
        &idx_args,
        &mut fuel,
    )
    .expect("kernel test value");
    drop_all(&mut heap, idx_args);
    assert_eq!(heap.tag(&idx), Ok("Prelude.Option.Some"));
    assert_eq!(heap.field(&idx, 0), Ok(&Value::Int(2)));

    // Lone surrogate encodes to UTF-8 U+FFFD [239, 191, 189]
    let encoded = eval_prim(
        &mut heap,
        Intrinsic::Bytes(BytesIntrinsic::EncodeUtf8),
        slice::from_ref(&high),
        &mut fuel,
    )
    .expect("kernel test value");
    let byte_list = eval_prim(
        &mut heap,
        Intrinsic::Bytes(BytesIntrinsic::ToList),
        slice::from_ref(&encoded),
        &mut fuel,
    )
    .expect("kernel test value");
    assert_eq!(heap.field(&byte_list, 0), Ok(&Value::Int(239)));
    assert_eq!(heap.field(&byte_list, 1), Ok(&Value::Int(191)));
    assert_eq!(heap.field(&byte_list, 2), Ok(&Value::Int(189)));

    drop_all(
        &mut heap,
        [earth, high, low, rejoined, globe, idx, encoded, byte_list],
    );
    heap.finish().expect("zero heap leaks");
}
