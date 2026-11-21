//! Counted `BigInt` primitive implementations (`packages/std/src/BigInt.fl`).

use flick_core::ir::Lit;
use flick_core::ir::memory::BigIntIntrinsic;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};

use super::{Error, Heap, Value, charge, normalize_bigint};

pub(super) fn run(
    heap: &mut Heap,
    op: BigIntIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        BigIntIntrinsic::FromNumber => from_number(heap, &args[0], false),
        BigIntIntrinsic::FromSafeNumber => from_number(heap, &args[0], true),
        BigIntIntrinsic::ToSafeNumber => to_safe_number(heap, &args[0], fuel),
        BigIntIntrinsic::ToString => {
            let text = heap.bigint(&args[0])?;
            let normalized = normalize_bigint(text, |work| charge(fuel, work))?;
            heap.literal(&Lit::String(normalized))
        }
        BigIntIntrinsic::Parse => parse_bigint(heap, &args[0], false, fuel),
        BigIntIntrinsic::FromString => parse_bigint(heap, &args[0], true, fuel),
        BigIntIntrinsic::Abs => {
            let n = parse_operand(heap, &args[0], fuel)?;
            heap.literal(&Lit::BigInt(n.abs().to_str_radix(10)))
        }
        BigIntIntrinsic::Sqrt => {
            let n = parse_operand(heap, &args[0], fuel)?;
            if n.is_negative() {
                heap.construct("Prelude.Option.None", vec![])
            } else {
                let root = heap.literal(&Lit::BigInt(n.sqrt().to_str_radix(10)))?;
                heap.construct("Prelude.Option.Some", vec![root])
            }
        }
        BigIntIntrinsic::Divide => checked_div_rem(heap, &args[0], &args[1], false, fuel),
        BigIntIntrinsic::Remainder => checked_div_rem(heap, &args[0], &args[1], true, fuel),
        BigIntIntrinsic::Gcd => {
            let a = parse_operand(heap, &args[1], fuel)?;
            let b = parse_operand(heap, &args[0], fuel)?;
            let g = gcd(a.abs(), b.abs(), fuel)?;
            heap.literal(&Lit::BigInt(g.to_str_radix(10)))
        }
        BigIntIntrinsic::Lcm => {
            let a = parse_operand(heap, &args[1], fuel)?;
            let b = parse_operand(heap, &args[0], fuel)?;
            let l = if a.is_zero() || b.is_zero() {
                BigInt::zero()
            } else {
                let aa = a.abs();
                let ba = b.abs();
                let g = gcd(aa.clone(), ba.clone(), fuel)?;
                (aa / g) * ba
            };
            heap.literal(&Lit::BigInt(l.to_str_radix(10)))
        }
    }
}

pub(super) fn parse_operand(heap: &Heap, value: &Value, fuel: &mut u64) -> Result<BigInt, Error> {
    let text = heap.bigint(value)?;
    let normalized = normalize_bigint(text, |work| charge(fuel, work))?;
    BigInt::parse_bytes(normalized.as_bytes(), 10)
        .ok_or_else(|| Error::InvalidBigInt(text.to_owned()))
}

fn gcd(mut a: BigInt, mut b: BigInt, fuel: &mut u64) -> Result<BigInt, Error> {
    while !b.is_zero() {
        charge(fuel, 1)?;
        let r = a % &b;
        a = b;
        b = r;
    }
    Ok(a)
}

fn from_number(heap: &mut Heap, value: &Value, safe_only: bool) -> Result<Value, Error> {
    let n = match *value {
        Value::Float(f) => f,
        Value::Int(i) => f64::from(i),
        _ => return Err(Error::ExpectedFloat),
    };
    let ok = if safe_only {
        super::float_ops::is_safe_integer(n)
    } else {
        n.is_finite() && n.fract() == 0.0
    };
    if !ok {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let digits = exact_integer_f64_digits(n);
    let val = heap.literal(&Lit::BigInt(digits))?;
    heap.construct("Prelude.Option.Some", vec![val])
}

/// Converts any finite integral `f64` (including values past `2^53`) to exact base-10 digits.
fn exact_integer_f64_digits(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    let bits = n.to_bits();
    let negative = (bits >> 63) != 0;
    let raw_exp = i32::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let exp = raw_exp - 1023 - 52;
    let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let mut big = BigInt::from(mantissa);
    if exp >= 0 {
        big <<= usize::try_from(exp).unwrap_or(0);
    } else {
        big >>= usize::try_from(-exp).unwrap_or(0);
    }
    if negative {
        big = -big;
    }
    big.to_str_radix(10)
}

fn to_safe_number(heap: &mut Heap, value: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let n = parse_operand(heap, value, fuel)?;
    let limit = BigInt::from(9_007_199_254_740_991_i64);
    if n.abs() <= limit
        && let Some(f) = n.to_f64()
    {
        heap.construct("Prelude.Option.Some", vec![Value::Float(f)])
    } else {
        heap.construct("Prelude.Option.None", vec![])
    }
}

fn parse_bigint(
    heap: &mut Heap,
    value: &Value,
    allow_radix: bool,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let text = heap.string(value)?;
    charge(fuel, text.len())?;
    let trimmed = super::string_ops::js_trim(text);
    let parsed = if trimmed.is_empty() {
        None
    } else if allow_radix
        && let Some(rest) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
    {
        parse_digits(rest, 16)
    } else if allow_radix
        && let Some(rest) = trimmed
            .strip_prefix("0o")
            .or_else(|| trimmed.strip_prefix("0O"))
    {
        parse_digits(rest, 8)
    } else if allow_radix
        && let Some(rest) = trimmed
            .strip_prefix("0b")
            .or_else(|| trimmed.strip_prefix("0B"))
    {
        parse_digits(rest, 2)
    } else {
        let digits = trimmed
            .strip_prefix('+')
            .or_else(|| trimmed.strip_prefix('-'))
            .unwrap_or(trimmed);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            BigInt::parse_bytes(trimmed.as_bytes(), 10)
        } else {
            None
        }
    };
    match parsed {
        Some(big) => {
            let val = heap.literal(&Lit::BigInt(big.to_str_radix(10)))?;
            heap.construct("Prelude.Option.Some", vec![val])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn parse_digits(digits: &str, radix: u32) -> Option<BigInt> {
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    BigInt::parse_bytes(digits.as_bytes(), radix)
}

fn checked_div_rem(
    heap: &mut Heap,
    left: &Value,
    right: &Value,
    rem: bool,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let a = parse_operand(heap, left, fuel)?;
    let b = parse_operand(heap, right, fuel)?;
    if b.is_zero() {
        let text = heap.literal(&Lit::String("division by zero".into()))?;
        let payload = heap.construct("Int.ArithmeticError.ArithmeticError", vec![text])?;
        return heap.construct("Prelude.Result.Err", vec![payload]);
    }
    let out = if rem { a % b } else { a / b };
    let val = heap.literal(&Lit::BigInt(out.to_str_radix(10)))?;
    heap.construct("Prelude.Result.Ok", vec![val])
}
