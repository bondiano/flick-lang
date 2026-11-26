//! Counted fixed-width integer and binary32 float primitive implementations
//! (`flick:rt` conversions, formatting, and 64-bit bitwise operations).

use flick_core::ir::Lit;
use flick_core::ir::memory::FixedWidthIntrinsic;

use super::{Error, Heap, Value, bigint_conversion, charge, normalize_bigint, scalar, unsupported};

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn run(
    heap: &mut Heap,
    op: FixedWidthIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        FixedWidthIntrinsic::ShowInt64 => {
            let Value::Int64(n) = args[0] else {
                return Err(unsupported("showInt64 expects Int64"));
            };
            charge(fuel, 20)?;
            heap.literal(&Lit::String(n.to_string()))
        }
        FixedWidthIntrinsic::ShowUInt64 => {
            let Value::UInt64(n) = args[0] else {
                return Err(unsupported("showUInt64 expects UInt64"));
            };
            charge(fuel, 20)?;
            heap.literal(&Lit::String(n.to_string()))
        }
        FixedWidthIntrinsic::ToInt8 => Ok(Value::Int8(to_u64_bits(heap, &args[0], fuel)? as i8)),
        FixedWidthIntrinsic::ToInt16 => Ok(Value::Int16(to_u64_bits(heap, &args[0], fuel)? as i16)),
        FixedWidthIntrinsic::ToInt32 => Ok(Value::Int(to_u64_bits(heap, &args[0], fuel)? as i32)),
        FixedWidthIntrinsic::ToInt64 => Ok(Value::Int64(to_u64_bits(heap, &args[0], fuel)? as i64)),
        FixedWidthIntrinsic::ToUInt8 => Ok(Value::UInt8(to_u64_bits(heap, &args[0], fuel)? as u8)),
        FixedWidthIntrinsic::ToUInt16 => {
            Ok(Value::UInt16(to_u64_bits(heap, &args[0], fuel)? as u16))
        }
        FixedWidthIntrinsic::ToUInt32 => {
            Ok(Value::UInt32(to_u64_bits(heap, &args[0], fuel)? as u32))
        }
        FixedWidthIntrinsic::ToUInt64 => Ok(Value::UInt64(to_u64_bits(heap, &args[0], fuel)?)),
        FixedWidthIntrinsic::ToFloat32 => to_float32(heap, &args[0], fuel),
        FixedWidthIntrinsic::ToFloat64 => to_float64(heap, &args[0], fuel),
        FixedWidthIntrinsic::ToBigInt => bigint_conversion::run(heap, &args[0], fuel),
        FixedWidthIntrinsic::Int64BitNot => {
            let Value::Int64(a) = args[0] else {
                return Err(unsupported("int64BitNot expects Int64"));
            };
            Ok(Value::Int64(!a))
        }
        FixedWidthIntrinsic::Int64BitAnd
        | FixedWidthIntrinsic::Int64BitOr
        | FixedWidthIntrinsic::Int64BitXor => {
            let (Value::Int64(a), Value::Int64(b)) = (&args[0], &args[1]) else {
                return Err(unsupported("int64 bitwise op expects two Int64s"));
            };
            let out = match op {
                FixedWidthIntrinsic::Int64BitAnd => a & b,
                FixedWidthIntrinsic::Int64BitOr => a | b,
                FixedWidthIntrinsic::Int64BitXor => a ^ b,
                _ => unreachable!(),
            };
            Ok(Value::Int64(out))
        }
        FixedWidthIntrinsic::Int64ShiftLeft
        | FixedWidthIntrinsic::Int64ShiftRight
        | FixedWidthIntrinsic::Int64ShiftRightUnsigned => int64_shift(op, &args[0], &args[1]),
    }
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "64-bit shifts mask shift count modulo 64"
)]
fn int64_shift(op: FixedWidthIntrinsic, left: &Value, right: &Value) -> Result<Value, Error> {
    let (Value::Int64(a), Value::Int(n)) = (left, right) else {
        return Err(unsupported("int64 shift expects (Int64, Int)"));
    };
    let shift = (*n as u32) & 63;
    let out = match op {
        FixedWidthIntrinsic::Int64ShiftLeft => a.wrapping_shl(shift),
        FixedWidthIntrinsic::Int64ShiftRight => a.wrapping_shr(shift),
        FixedWidthIntrinsic::Int64ShiftRightUnsigned => ((*a as u64).wrapping_shr(shift)) as i64,
        _ => unreachable!(),
    };
    Ok(Value::Int64(out))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact bit-preserving modular reduction for finite f64"
)]
fn f64_to_u64_wrapping(x: f64) -> u64 {
    if !x.is_finite() || x == 0.0 {
        return 0;
    }
    let m = x.trunc() % 18_446_744_073_709_551_616.0;
    let mag = m.abs() as u64;
    if m < 0.0 { mag.wrapping_neg() } else { mag }
}

fn decimal_bigint_to_u64_wrapping(text: &str, fuel: &mut u64) -> Result<u64, Error> {
    let normalized = normalize_bigint(text, |work| charge(fuel, work))?;
    let (negative, digits) = normalized
        .strip_prefix('-')
        .map_or((false, normalized.as_str()), |s| (true, s));
    charge(fuel, digits.len())?;
    let mut low = 0_u64;
    for byte in digits.bytes() {
        low = low.wrapping_mul(10).wrapping_add(u64::from(byte - b'0'));
    }
    Ok(if negative { low.wrapping_neg() } else { low })
}

#[expect(
    clippy::cast_sign_loss,
    reason = "two's-complement bit-preserving modular conversion"
)]
fn to_u64_bits(heap: &Heap, value: &Value, fuel: &mut u64) -> Result<u64, Error> {
    match value {
        Value::Int(n) => Ok(i64::from(*n) as u64),
        Value::Int8(n) => Ok(i64::from(*n) as u64),
        Value::Int16(n) => Ok(i64::from(*n) as u64),
        Value::Int64(n) => Ok(*n as u64),
        Value::UInt8(n) => Ok(u64::from(*n)),
        Value::UInt16(n) => Ok(u64::from(*n)),
        Value::UInt32(n) => Ok(u64::from(*n)),
        Value::UInt64(n) => Ok(*n),
        Value::Float(x) => Ok(f64_to_u64_wrapping(*x)),
        Value::Float32(x) => Ok(f64_to_u64_wrapping(f64::from(*x))),
        Value::Object(_) => decimal_bigint_to_u64_wrapping(heap.bigint(value)?, fuel),
        _ => Err(unsupported("fixed-width integer conversion operand")),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "IEEE 754 single-rounded conversion to binary32"
)]
fn to_float32(heap: &Heap, value: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let f = match value {
        Value::Int(n) => *n as f32,
        Value::Int8(n) => f32::from(*n),
        Value::Int16(n) => f32::from(*n),
        Value::Int64(n) => *n as f32,
        Value::UInt8(n) => f32::from(*n),
        Value::UInt16(n) => f32::from(*n),
        Value::UInt32(n) => *n as f32,
        Value::UInt64(n) => *n as f32,
        Value::Float(x) => *x as f32,
        Value::Float32(x) => *x,
        Value::Object(_) => scalar::bigint_float32(heap.bigint(value)?, fuel)?,
        _ => return Err(unsupported("toFloat32 operand")),
    };
    Ok(Value::Float32(f))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "IEEE 754 rounding from 64-bit integers to binary64"
)]
fn to_float64(heap: &Heap, value: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let f = match value {
        Value::Int(n) => f64::from(*n),
        Value::Int8(n) => f64::from(*n),
        Value::Int16(n) => f64::from(*n),
        Value::Int64(n) => *n as f64,
        Value::UInt8(n) => f64::from(*n),
        Value::UInt16(n) => f64::from(*n),
        Value::UInt32(n) => f64::from(*n),
        Value::UInt64(n) => *n as f64,
        Value::Float(x) => *x,
        Value::Float32(x) => f64::from(*x),
        Value::Object(_) => scalar::bigint_float(heap.bigint(value)?, fuel)?,
        _ => return Err(unsupported("toFloat64 operand")),
    };
    Ok(Value::Float(f))
}
