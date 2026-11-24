//! ECMA-262 2025 global `BigInt` over audited primitive language domains.
//! Objects need their actual `ToPrimitive` protocol, not guessed coercions.
use std::{fmt::Write as _, iter};

use super::super::{Data, Error, Heap, Value, charge};
use super::{normalize_bigint, unsupported};

pub(super) fn run(heap: &mut Heap, input: &Value, fuel: &mut u64) -> Result<Value, Error> {
    charge(fuel, 1)?;
    let text = match input {
        Value::Int(value) => {
            // Every i32 decimal spelling fits in eleven bytes, including sign.
            charge(fuel, 11)?;
            value.to_string()
        }
        Value::Int8(value) => {
            charge(fuel, 4)?;
            value.to_string()
        }
        Value::Int16(value) => {
            charge(fuel, 6)?;
            value.to_string()
        }
        Value::Int64(value) => {
            charge(fuel, 20)?;
            value.to_string()
        }
        Value::UInt8(value) => {
            charge(fuel, 3)?;
            value.to_string()
        }
        Value::UInt16(value) => {
            charge(fuel, 5)?;
            value.to_string()
        }
        Value::UInt32(value) => {
            charge(fuel, 10)?;
            value.to_string()
        }
        Value::UInt64(value) => {
            charge(fuel, 20)?;
            value.to_string()
        }
        Value::Bool(value) => {
            charge(fuel, 1)?;
            if *value { "1".into() } else { "0".into() }
        }
        Value::Float(value) => number(*value, fuel)?,
        Value::Float32(value) => number(f64::from(*value), fuel)?,
        Value::Unit => return Err(Error::BigIntUndefined),
        Value::Object(token) => match &heap.object(token.id)?.data {
            Data::BigInt(_) => return heap.dup(input),
            Data::String(value) => string(value, fuel)?,
            _ => {
                return Err(unsupported(
                    "BigInt object conversion requires ToPrimitive support",
                ));
            }
        },
        _ => {
            return Err(unsupported(
                "BigInt object conversion requires ToPrimitive support",
            ));
        }
    };
    charge(fuel, 1)?;
    Ok(heap.allocate(Data::BigInt(text)))
}

fn number(value: f64, fuel: &mut u64) -> Result<String, Error> {
    charge(fuel, 1)?;
    let bits = value.to_bits();
    let exponent = u16::try_from((bits >> 52) & 0x7ff).expect("eleven exponent bits");
    let fraction = bits & ((1_u64 << 52) - 1);
    if exponent == 0 && fraction == 0 {
        charge(fuel, 1)?;
        return Ok("0".into());
    }
    if exponent == 0x7ff || exponent < 1023 {
        return Err(Error::BigIntNonIntegralNumber);
    }
    let power = i32::from(exponent) - 1023 - 52;
    let mut significand = (1_u64 << 52) | fraction;
    let zeroes = if power < 0 {
        let shift = u32::try_from(-power).expect("nonnegative shift at most 52");
        if significand & ((1_u64 << shift) - 1) != 0 {
            return Err(Error::BigIntNonIntegralNumber);
        }
        significand >>= shift;
        0
    } else {
        // At most 53 + 3 bits fit in u64. Zero hex places supply the rest.
        significand <<= power % 4;
        usize::try_from(power / 4).expect("nonnegative bounded binary64 exponent")
    };
    let negative = bits >> 63 != 0;
    // Prepay the bounded integer formatter and every appended zero/sign byte.
    charge(fuel, 17_usize.saturating_add(zeroes))?;
    let mut hexadecimal = String::with_capacity(17 + zeroes);
    hexadecimal.push_str(if negative { "-0x" } else { "0x" });
    write!(hexadecimal, "{significand:x}").expect("writing into a String cannot fail");
    hexadecimal.extend(iter::repeat_n('0', zeroes));
    normalize_bigint(&hexadecimal, |work| charge(fuel, work))
}

fn string(value: &str, fuel: &mut u64) -> Result<String, Error> {
    charge(fuel, value.len())?;
    let trimmed = value.trim_matches(js_space);
    if trimmed.is_empty() {
        charge(fuel, 1)?;
        return Ok("0".into());
    }
    let (signed, unsigned) = trimmed
        .strip_prefix('+')
        .or_else(|| trimmed.strip_prefix('-'))
        .map_or((false, trimmed), |text| (true, text));
    let radix = unsigned.starts_with("0x")
        || unsigned.starts_with("0X")
        || unsigned.starts_with("0b")
        || unsigned.starts_with("0B")
        || unsigned.starts_with("0o")
        || unsigned.starts_with("0O");
    if signed {
        charge(fuel, unsigned.len())?;
        if radix || unsigned.is_empty() || !unsigned.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::BigIntInvalidString);
        }
    }
    // Only decimal allows a sign. '+' is not a literal-normalizer prefix.
    let literal = trimmed.strip_prefix('+').unwrap_or(trimmed);
    normalize_bigint(literal, |work| charge(fuel, work)).map_err(|error| match error {
        Error::InvalidBigInt(_) => Error::BigIntInvalidString,
        error => error,
    })
}

// WhiteSpace and LineTerminator from ECMA-262 2025, not Rust's wider Unicode set.
pub(super) fn js_space(character: char) -> bool {
    matches!(character,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
