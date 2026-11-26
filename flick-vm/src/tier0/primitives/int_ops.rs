//! Counted `Int` primitive implementations (`packages/core/src/Int.fl`).

use flick_core::ir::Lit;
use flick_core::ir::memory::IntIntrinsic;

use super::{Error, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: IntIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        IntIntrinsic::Parse => {
            let text = heap.string(&args[0])?;
            charge(fuel, text.len())?;
            let trimmed = text.trim_matches(super::bigint_conversion::js_space);
            let parsed = parse_decimal_i32(trimmed);
            match parsed {
                Some(n) => heap.construct("Prelude.Option.Some", vec![Value::Int(n)]),
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        IntIntrinsic::FromFloat => {
            let Value::Float(n) = args[0] else {
                return Err(Error::ExpectedFloat);
            };
            if let Some(i) = exact_i32_from_f64(n) {
                heap.construct("Prelude.Option.Some", vec![Value::Int(i)])
            } else {
                heap.construct("Prelude.Option.None", vec![])
            }
        }
        IntIntrinsic::Truncate => {
            let Value::Float(n) = args[0] else {
                return Err(Error::ExpectedFloat);
            };
            Ok(Value::Int(js_to_int32(n)))
        }
        IntIntrinsic::Abs => {
            let Value::Int(n) = args[0] else {
                return Err(Error::ExpectedInt);
            };
            Ok(Value::Int(n.wrapping_abs()))
        }
        IntIntrinsic::BitNot => {
            let Value::Int(n) = args[0] else {
                return Err(Error::ExpectedInt);
            };
            Ok(Value::Int(!n))
        }
        IntIntrinsic::CheckedAdd
        | IntIntrinsic::CheckedSub
        | IntIntrinsic::CheckedMul
        | IntIntrinsic::CheckedDiv
        | IntIntrinsic::CheckedRem => checked_arith(heap, op, &args[0], &args[1]),
        IntIntrinsic::BitAnd
        | IntIntrinsic::BitOr
        | IntIntrinsic::BitXor
        | IntIntrinsic::ShiftLeft
        | IntIntrinsic::ShiftRight
        | IntIntrinsic::ShiftRightUnsigned => bitwise_binary(op, &args[0], &args[1]),
    }
}

fn parse_decimal_i32(text: &str) -> Option<i32> {
    if text.is_empty() {
        return None;
    }
    let digits = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed: i64 = text.parse().ok()?;
    i32::try_from(parsed).ok()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "bounds check guarantees exact i32 representation"
)]
fn exact_i32_from_f64(n: f64) -> Option<i32> {
    if !n.is_finite() || n.fract() != 0.0 {
        return None;
    }
    if n >= f64::from(i32::MIN) && n <= f64::from(i32::MAX) {
        Some(if n == 0.0 { 0 } else { n as i32 })
    } else {
        None
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "ECMAScript ToInt32 wraps truncated finite f64 modulo 2^32"
)]
pub(super) fn js_to_int32(n: f64) -> i32 {
    if !n.is_finite() || n == 0.0 {
        return 0;
    }
    let wrapped = n.trunc().rem_euclid(4_294_967_296.0);
    (wrapped as u32) as i32
}

fn arithmetic_err(heap: &mut Heap, message: String) -> Result<Value, Error> {
    let text = heap.literal(&Lit::String(message))?;
    let payload = heap.construct("Int.ArithmeticError.ArithmeticError", vec![text])?;
    heap.construct("Prelude.Result.Err", vec![payload])
}

fn checked_arith(
    heap: &mut Heap,
    op: IntIntrinsic,
    left: &Value,
    right: &Value,
) -> Result<Value, Error> {
    let (Value::Int(a), Value::Int(b)) = (left, right) else {
        return Err(Error::ExpectedInt);
    };
    let (a64, b64) = (i64::from(*a), i64::from(*b));
    let (name, res) = match op {
        IntIntrinsic::CheckedAdd => ("checked_add", a64 + b64),
        IntIntrinsic::CheckedSub => ("checked_sub", a64 - b64),
        IntIntrinsic::CheckedMul => ("checked_mul", a64 * b64),
        IntIntrinsic::CheckedDiv => {
            if *b == 0 {
                return arithmetic_err(heap, "Int.checked_div: division by zero".into());
            }
            ("checked_div", a64 / b64)
        }
        IntIntrinsic::CheckedRem => {
            if *b == 0 {
                return arithmetic_err(heap, "Int.checked_rem: division by zero".into());
            }
            ("checked_rem", a64 % b64)
        }
        _ => unreachable!("checked_arith dispatch"),
    };
    if let Ok(fitted) = i32::try_from(res) {
        heap.construct("Prelude.Result.Ok", vec![Value::Int(fitted)])
    } else {
        arithmetic_err(heap, format!("Int.{name}: {res} does not fit in Int"))
    }
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "ECMAScript bitwise shifts operate on 32-bit unsigned shift counts and bit patterns"
)]
fn bitwise_binary(op: IntIntrinsic, left: &Value, right: &Value) -> Result<Value, Error> {
    let (Value::Int(a), Value::Int(b)) = (left, right) else {
        return Err(Error::ExpectedInt);
    };
    let out = match op {
        IntIntrinsic::BitAnd => a & b,
        IntIntrinsic::BitOr => a | b,
        IntIntrinsic::BitXor => a ^ b,
        IntIntrinsic::ShiftLeft => a.wrapping_shl(*b as u32),
        IntIntrinsic::ShiftRight => a.wrapping_shr(*b as u32),
        IntIntrinsic::ShiftRightUnsigned => ((*a as u32).wrapping_shr(*b as u32)) as i32,
        _ => unreachable!("bitwise_binary dispatch"),
    };
    Ok(Value::Int(out))
}
