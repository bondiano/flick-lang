//! Scalar operations borrow staged operands; no host evaluation or float parser.
use super::super::{Error, Heap, Value, charge};
use super::{normalize_bigint, unsupported};
use flick_core::ir::memory::Intrinsic;
use std::cmp::Ordering;

pub(super) fn run(
    heap: &Heap,
    kind: Intrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    match kind {
        Intrinsic::NumberToFloat => match &args[0] {
            Value::Int(value) => Ok(Value::Float(f64::from(*value))),
            Value::Float(value) => Ok(Value::Float(*value)),
            Value::Object(_) => bigint_float(heap.bigint(&args[0])?, fuel).map(Value::Float),
            _ => Err(unsupported(
                "Number conversion requires an Int, BigInt or Float",
            )),
        },
        Intrinsic::FloatMin | Intrinsic::FloatMax => select(kind, &args[0], &args[1]),
        _ => Err(unsupported("not a scalar conversion or selection")),
    }
}

fn select(kind: Intrinsic, that: &Value, this: &Value) -> Result<Value, Error> {
    let (Value::Float(that), Value::Float(this)) = (that, this) else {
        return Err(unsupported("Float min/max requires two Float operands"));
    };
    // Pinned Effect Number.Order: NaN is least; two NaNs and signed zeros
    // compare equal. Data-last min/max select `self` (the SECOND source
    // operand) on equality, preserving its original zero sign / NaN bits.
    let order = match (this.is_nan(), that.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => this.partial_cmp(that).expect("non-NaN order"),
    };
    let keep_self = if kind == Intrinsic::FloatMin {
        order != Ordering::Greater
    } else {
        order != Ordering::Less
    };
    Ok(Value::Float(if keep_self { *this } else { *that }))
}

fn bigint_float(text: &str, fuel: &mut u64) -> Result<f64, Error> {
    let normalized = normalize_bigint(text, |work| charge(fuel, work))?;
    let (negative, digits) = normalized
        .strip_prefix('-')
        .map_or((false, normalized.as_str()), |s| (true, s));
    let sign = if negative { 1_u64 << 63 } else { 0 };
    // All validation, radix conversion and leading-zero removal has completed.
    // Any 310-digit integer is >= 10^309 > binary64's largest finite value.
    if digits.len() > 309 {
        return Ok(f64::from_bits(sign | 0x7ff0_0000_0000_0000));
    }
    let bits = binary_digits(digits, fuel)?;
    if bits.is_empty() {
        return Ok(0.0);
    }
    rounded(&bits, sign, fuel)
}

/// Least-significant bit first, via exact base-2^32 decimal accumulation.
/// Each word update/carry step and emitted bit is charged BEFORE work.
fn binary_digits(decimal: &str, fuel: &mut u64) -> Result<Vec<u8>, Error> {
    charge(fuel, decimal.len())?;
    if decimal == "0" {
        return Ok(vec![]);
    }
    let mut words = vec![0_u32];
    for digit in decimal.bytes() {
        charge(fuel, words.len().saturating_add(1))?;
        let mut carry = u64::from(digit - b'0');
        for word in &mut words {
            // word <=2^32-1 and carry<=9, so this is <2^36.
            let value = u64::from(*word) * 10 + carry;
            *word = u32::try_from(value & u64::from(u32::MAX)).expect("low32 bits");
            carry = value >> 32;
        }
        if carry != 0 {
            words.push(u32::try_from(carry).expect("decimal carry <=9"));
        }
    }
    let mut bits = Vec::new();
    for word in words {
        charge(fuel, 32)?;
        for shift in 0..32 {
            bits.push(u8::from(word & (1 << shift) != 0));
        }
    }
    while bits.last() == Some(&0) {
        charge(fuel, 1)?;
        bits.pop();
    }
    Ok(bits)
}

fn rounded(bits: &[u8], sign: u64, fuel: &mut u64) -> Result<f64, Error> {
    // 309 decimal digits have at most 1,027 binary digits, so this checked
    // representation conversion cannot overflow a u64 on any Rust target.
    let mut exponent = u64::try_from(bits.len() - 1).expect("bounded canonical integer");
    if exponent > 1023 {
        return Ok(f64::from_bits(sign | 0x7ff0_0000_0000_0000));
    }
    charge(fuel, 53)?;
    let mut significand = bits
        .iter()
        .rev()
        .take(53)
        .fold(0_u64, |value, bit| (value << 1) | u64::from(*bit));
    for _ in bits.len()..53 {
        significand <<= 1;
    }
    if bits.len() > 53 {
        let discarded = bits.len() - 53;
        charge(fuel, discarded)?;
        let guard = bits[discarded - 1] != 0;
        let sticky = bits[..discarded - 1].iter().any(|bit| *bit != 0);
        if guard && (sticky || significand & 1 != 0) {
            significand += 1;
            if significand == 1_u64 << 53 {
                significand >>= 1;
                exponent += 1;
            }
        }
    }
    if exponent > 1023 {
        return Ok(f64::from_bits(sign | 0x7ff0_0000_0000_0000));
    }
    Ok(f64::from_bits(
        sign | ((exponent + 1023) << 52) | (significand & 0x000f_ffff_ffff_ffff),
    ))
}
