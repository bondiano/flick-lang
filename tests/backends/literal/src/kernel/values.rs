//! Primitive values as the kernel's primitives read and make them: code
//! units of strings, integers of any width, conversions, the total order,
//! hashes and the text of numbers and strings.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::rc::Rc;

use flick_backend_support::opt::eval::Value;
use flick_backend_support::opt::js_number;

pub(super) fn text<'a>(s: &str) -> Value<'a> {
    Value::String(Rc::from(s))
}

/// The exports [`bits`] reads.
pub(super) const BITS: &[&str] = &[
    "bitAnd",
    "bitOr",
    "bitXor",
    "shiftLeft",
    "shiftRight",
    "shiftRightUnsignedInt",
    "int64BitAnd",
    "int64BitOr",
    "int64BitXor",
    "int64ShiftLeft",
    "int64ShiftRight",
    "int64ShiftRightUnsigned",
];

#[allow(
    clippy::cast_possible_truncation,
    reason = "a whole number below 2^127"
)]
pub(super) fn whole_number(x: f64) -> i128 {
    x as i128
}

/// The UTF-16 code units of a string value.
pub(super) fn units(v: &Value<'_>) -> Option<Vec<u16>> {
    match v {
        Value::String(s) => Some(s.encode_utf16().collect()),
        Value::Utf16(u) => Some(u.to_vec()),
        _ => None,
    }
}

/// A string of code units, kept as units when a surrogate is alone.
pub(super) fn from_units<'a>(units: &[u16]) -> Value<'a> {
    String::from_utf16(units).map_or_else(|_| Value::Utf16(Rc::new(units.to_vec())), |s| text(&s))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a BigInt rounds to the nearest Float"
)]
pub(super) fn float(v: &Value<'_>) -> Option<f64> {
    Some(match v {
        Value::Int(n) => f64::from(*n),
        Value::BigInt(n) => *n as f64,
        _ => return None,
    })
}

/// An integer value of any width.
pub(super) fn integer(v: &Value<'_>) -> Option<i128> {
    Some(match v {
        Value::Int(n) => (*n).into(),
        Value::Int8(n) => (*n).into(),
        Value::Int16(n) => (*n).into(),
        Value::Int64(n) => (*n).into(),
        Value::UInt8(n) => (*n).into(),
        Value::UInt16(n) => (*n).into(),
        Value::UInt32(n) => (*n).into(),
        Value::UInt64(n) => (*n).into(),
        Value::BigInt(n) => *n,
        _ => return None,
    })
}

/// ECMAScript `ToInt32`: the whole part, modulo 2^32; NaN and infinities are 0.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "the remainder is a whole number in 0..2^32"
)]
pub(super) fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}

/// A conversion between numeric types: wrapping to an integer width, exact
/// to `BigInt`, rounded once to a float.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "conversions wrap and round by definition"
)]
pub(super) fn convert<'a>(export: &str, v: &Value<'_>) -> Option<Value<'a>> {
    if let Some(n) = integer(v) {
        return Some(match export {
            "toInt8" => Value::Int8(n as i8),
            "toInt16" => Value::Int16(n as i16),
            "toInt32" => Value::Int(n as i32),
            "toInt64" => Value::Int64(n as i64),
            "toUInt8" => Value::UInt8(n as u8),
            "toUInt16" => Value::UInt16(n as u16),
            "toUInt32" => Value::UInt32(n as u32),
            "toUInt64" => Value::UInt64(n as u64),
            "toBigInt" => Value::BigInt(n),
            "toFloat32" => Value::Float32(n as f32),
            "toFloat64" => Value::Float(n as f64),
            _ => return None,
        });
    }
    Some(match (export, v) {
        ("toFloat32", Value::Float(x)) => Value::Float32(*x as f32),
        ("toFloat64", Value::Float32(x)) => Value::Float(f64::from(*x)),
        _ => return None,
    })
}

/// The binary bit operations: counts are masked to the width.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "shift counts are masked and unsigned shifts read the bits back"
)]
pub(super) fn bits<'a>(export: &str, a: &Value<'_>, b: &Value<'_>) -> Option<Value<'a>> {
    Some(match (export, a, b) {
        ("bitAnd", Value::Int(a), Value::Int(b)) => Value::Int(a & b),
        ("bitOr", Value::Int(a), Value::Int(b)) => Value::Int(a | b),
        ("bitXor", Value::Int(a), Value::Int(b)) => Value::Int(a ^ b),
        ("shiftLeft", Value::Int(a), Value::Int(b)) => Value::Int(a.wrapping_shl(*b as u32)),
        ("shiftRight", Value::Int(a), Value::Int(b)) => Value::Int(a.wrapping_shr(*b as u32)),
        ("shiftRightUnsignedInt", Value::Int(a), Value::Int(b)) => {
            Value::Int((*a as u32).wrapping_shr(*b as u32) as i32)
        }
        ("int64BitAnd", Value::Int64(a), Value::Int64(b)) => Value::Int64(a & b),
        ("int64BitOr", Value::Int64(a), Value::Int64(b)) => Value::Int64(a | b),
        ("int64BitXor", Value::Int64(a), Value::Int64(b)) => Value::Int64(a ^ b),
        ("int64ShiftLeft", Value::Int64(a), Value::Int(b)) => {
            Value::Int64(a.wrapping_shl(*b as u32))
        }
        ("int64ShiftRight", Value::Int64(a), Value::Int(b)) => {
            Value::Int64(a.wrapping_shr(*b as u32))
        }
        ("int64ShiftRightUnsigned", Value::Int64(a), Value::Int(b)) => {
            Value::Int64((*a as u64).wrapping_shr(*b as u32) as i64)
        }
        _ => return None,
    })
}

/// A primitive value as a float, for `SameValueZero` and the total order.
pub(super) fn number(v: &Value<'_>) -> Option<f64> {
    match v {
        Value::Float(x) => Some(*x),
        Value::Float32(x) => Some(f64::from(*x)),
        _ => None,
    }
}

/// `==` of primitive values: `SameValueZero` on floats, code units on strings.
#[allow(clippy::float_cmp, reason = "SameValueZero is exact")]
pub(super) fn equal(a: &Value<'_>, b: &Value<'_>) -> Option<bool> {
    if let (Some(x), Some(y)) = (number(a), number(b)) {
        return Some(x == y || x.is_nan() && y.is_nan());
    }
    if let (Some(x), Some(y)) = (integer(a), integer(b)) {
        return Some(x == y);
    }
    match (a, b) {
        (Value::Bool(x), Value::Bool(y)) => Some(x == y),
        (Value::Con { ctor: x, .. }, Value::Con { ctor: y, .. }) => Some(x.name == y.name),
        _ => Some(units(a)? == units(b)?),
    }
}

/// The total order: NaN above every float, -0 equal to 0, strings by units.
pub(super) fn order(a: &Value<'_>, b: &Value<'_>) -> Option<i8> {
    let ordering = if let (Some(x), Some(y)) = (number(a), number(b)) {
        match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => x.partial_cmp(&y)?,
        }
    } else if let (Some(x), Some(y)) = (integer(a), integer(b)) {
        x.cmp(&y)
    } else if let (Value::Bool(x), Value::Bool(y)) = (a, b) {
        x.cmp(y)
    } else {
        units(a)?.cmp(&units(b)?)
    };
    Some(ordering as i8)
}

/// A hash that agrees with [`equal`]: equal values give one number.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a hash keeps the low bits"
)]
pub(super) fn hash(v: &Value<'_>) -> Option<i32> {
    let mut h: u32 = 5381;
    let mut mix = |x: u32| h = h.wrapping_mul(33) ^ x;
    if let Some(x) = number(v) {
        let x = if x.is_nan() { f64::NAN } else { x + 0.0 };
        let bits = x.to_bits();
        mix(bits as u32);
        mix((bits >> 32) as u32);
    } else if let Some(n) = integer(v) {
        mix(n as u32);
        mix((n >> 32) as u32);
        mix((n >> 64) as u32);
        mix((n >> 96) as u32);
    } else if let Value::Bool(b) = v {
        mix(u32::from(*b));
    } else {
        units(v)?.into_iter().for_each(|u| mix(u.into()));
    }
    Some(i32::from_ne_bytes(h.to_ne_bytes()))
}

/// `show` of a primitive value, as a JS literal of it reads.
pub(super) fn show(v: &Value<'_>) -> Option<String> {
    Some(match v {
        Value::Bool(b) => b.to_string(),
        Value::Float(x) => js_number(*x),
        Value::Float32(x) => js_number((*x).into()),
        Value::BigInt(n) => format!("{n}n"),
        Value::Con { ctor, args, .. } if args.is_empty() => ctor.name.clone(),
        Value::String(_) | Value::Utf16(_) => quote(&units(v)?),
        _ => integer(v)?.to_string(),
    })
}

/// `JSON.stringify` of code units: a lone surrogate as a lowercase `\u` escape.
pub(super) fn quote(units: &[u16]) -> String {
    let mut out = String::from("\"");
    for c in char::decode_utf16(units.iter().copied()) {
        match c {
            Ok('"') => out.push_str("\\\""),
            Ok('\\') => out.push_str("\\\\"),
            Ok('\n') => out.push_str("\\n"),
            Ok('\r') => out.push_str("\\r"),
            Ok('\t') => out.push_str("\\t"),
            Ok('\u{8}') => out.push_str("\\b"),
            Ok('\u{c}') => out.push_str("\\f"),
            Ok(c) if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            Ok(c) => out.push(c),
            Err(e) => {
                let _ = write!(out, "\\u{:04x}", e.unpaired_surrogate());
            }
        }
    }
    out.push('"');
    out
}

/// JS `Number(text)` of trimmed text that is not blank, or `None` when it
/// reads as no number (but for `NaN` itself).
#[allow(clippy::cast_precision_loss, reason = "rounds as JS does below 2^128")]
pub(super) fn to_number(units: &[u16]) -> Option<f64> {
    let text = String::from_utf16(units).ok()?;
    let text = text.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200A}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202F}'
                    | '\u{205F}'
                    | '\u{3000}'
                    | '\u{FEFF}'
        )
    });
    if text == "NaN" {
        return Some(f64::NAN);
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return u128::from_str_radix(digits, radix)
                .ok()
                .filter(|_| !digits.starts_with(['+', '-']))
                .map(|n| n as f64);
        }
    }
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(text)),
    };
    if rest == "Infinity" {
        return Some(sign * f64::INFINITY);
    }
    let decimal = rest.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
    decimal
        .then(|| rest.parse::<f64>().ok())
        .flatten()
        .map(|x| sign * x)
}
