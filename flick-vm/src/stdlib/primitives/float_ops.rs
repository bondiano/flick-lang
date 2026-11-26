//! Counted `Float` / `Math` / `Number` primitive implementations (`packages/core/src/Float.fl`).

use std::f64::consts::LN_2;

use flick_core::ir::Lit;
use flick_core::ir::memory::FloatIntrinsic;
use flick_core::opt::js_number;
use num_bigint::BigInt;
use num_traits::Zero;

use super::{Error, Fuel, Heap, Value, charge};

const MAX_SAFE_INT: f64 = 9_007_199_254_740_991.0;

pub(super) fn run(
    heap: &mut Heap,
    op: FloatIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        FloatIntrinsic::IsNaN => Ok(Value::Bool(expect_f64(&args[0])?.is_nan())),
        FloatIntrinsic::IsFinite => Ok(Value::Bool(expect_f64(&args[0])?.is_finite())),
        FloatIntrinsic::IsInteger => {
            let n = expect_f64(&args[0])?;
            Ok(Value::Bool(n.is_finite() && n.fract() == 0.0))
        }
        FloatIntrinsic::IsSafeInteger => {
            let n = expect_f64(&args[0])?;
            Ok(Value::Bool(is_safe_integer(n)))
        }
        FloatIntrinsic::Abs => Ok(Value::Float(expect_f64(&args[0])?.abs())),
        FloatIntrinsic::Floor => Ok(Value::Float(expect_f64(&args[0])?.floor())),
        FloatIntrinsic::Ceil => Ok(Value::Float(expect_f64(&args[0])?.ceil())),
        FloatIntrinsic::Round => Ok(Value::Float(js_math_round(expect_f64(&args[0])?))),
        FloatIntrinsic::Trunc => Ok(Value::Float(expect_f64(&args[0])?.trunc())),
        FloatIntrinsic::Sqrt => Ok(Value::Float(expect_f64(&args[0])?.sqrt())),
        FloatIntrinsic::NextPow2 => {
            let n = expect_f64(&args[0])?;
            let exp = (n.ln() / LN_2).ceil();
            let pow = 2.0_f64.powf(exp);
            let out = if pow.is_nan() { f64::NAN } else { pow.max(2.0) };
            Ok(Value::Float(out))
        }
        FloatIntrinsic::SafeInteger => {
            let n = expect_f64(&args[0])?;
            if is_safe_integer(n) {
                heap.construct("Prelude.Option.Some", vec![Value::Float(n)])
            } else {
                heap.construct("Prelude.Option.None", vec![])
            }
        }
        FloatIntrinsic::ToString => {
            let text = match args[0] {
                Value::Int(i) => i.to_string(),
                Value::Float(n) => {
                    if n == 0.0 && n.is_sign_negative() {
                        "-0".into()
                    } else {
                        js_number(n)
                    }
                }
                _ => return Err(Error::ExpectedFloat),
            };
            heap.literal(&Lit::String(text))
        }
        FloatIntrinsic::Parse => {
            let text = heap.string(&args[0])?;
            charge(fuel, text.len())?;
            let parsed = match text {
                "NaN" => Some(f64::NAN),
                "Infinity" => Some(f64::INFINITY),
                "-Infinity" => Some(f64::NEG_INFINITY),
                _ => parse_js_number(text, false),
            };
            option_float(heap, parsed)
        }
        FloatIntrinsic::StringToNumber => {
            let text = heap.string(&args[0])?;
            charge(fuel, text.len())?;
            option_float(heap, parse_js_number(text, true))
        }
        FloatIntrinsic::Remainder => {
            // Curried subject-last: args = [divisor, self]
            let divisor = expect_f64(&args[0])?;
            let self_val = expect_f64(&args[1])?;
            Ok(Value::Float(decimal_remainder(self_val, divisor)))
        }
        FloatIntrinsic::RoundTo => {
            // Curried subject-last: args = [precision, self]
            let precision = expect_f64(&args[0])?;
            let self_val = expect_f64(&args[1])?;
            let factor = 10.0_f64.powf(precision);
            Ok(Value::Float(js_math_round(self_val * factor) / factor))
        }
    }
}

fn option_float(heap: &mut Heap, value: Option<f64>) -> Result<Value, Error> {
    match value {
        Some(n) => heap.construct("Prelude.Option.Some", vec![Value::Float(n)]),
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn expect_f64(value: &Value) -> Result<f64, Error> {
    match *value {
        Value::Float(n) => Ok(n),
        Value::Int(i) => Ok(f64::from(i)),
        _ => Err(Error::ExpectedFloat),
    }
}

pub(super) fn is_safe_integer(n: f64) -> bool {
    n.is_finite() && n.fract() == 0.0 && n.abs() <= MAX_SAFE_INT
}

/// ECMAScript `Math.round`: ties toward `+Infinity`, preserving `-0.0` on `[-0.5, -0.0]`.
pub(super) fn js_math_round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 || x.fract() == 0.0 {
        return x;
    }
    if (-0.5..0.0).contains(&x) {
        return -0.0;
    }
    (x + 0.5).floor()
}

/// Parses JS `Number(s)` for non-empty trimmed text.
pub(super) fn parse_js_number(s: &str, allow_trimmed_nan: bool) -> Option<f64> {
    let text = super::string_ops::js_trim(s);
    if text.is_empty() {
        return None;
    }
    if text == "NaN" {
        return allow_trimmed_nan.then_some(f64::NAN);
    }
    if text == "Infinity" || text == "+Infinity" {
        return Some(f64::INFINITY);
    }
    if text == "-Infinity" {
        return Some(f64::NEG_INFINITY);
    }
    if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return parse_radix_f64(rest, 16);
    }
    if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        return parse_radix_f64(rest, 2);
    }
    if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        return parse_radix_f64(rest, 8);
    }
    if !is_js_decimal_literal(text) {
        return None;
    }
    text.parse::<f64>().ok()
}

fn parse_radix_f64(digits: &str, radix: u32) -> Option<f64> {
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let big = BigInt::parse_bytes(digits.as_bytes(), radix)?;
    let text = big.to_str_radix(10);
    text.parse::<f64>().ok()
}

fn is_js_decimal_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start_int = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let has_int = i > start_int;
    let mut has_frac = false;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let start_frac = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        has_frac = i > start_frac;
    }
    if !has_int && !has_frac {
        return false;
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        i += 1;
        if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
            i += 1;
        }
        let start_exp = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start_exp {
            return false;
        }
    }
    i == bytes.len()
}

fn to_scientific_int(n: f64) -> Option<(BigInt, i32)> {
    let repr = js_number(n.abs());
    let (mantissa, exp_part) = match repr.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (repr.as_str(), 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    let digits = format!("{int_part}{frac_part}");
    let mut coeff = BigInt::parse_bytes(digits.as_bytes(), 10)?;
    if n.is_sign_negative() {
        coeff = -coeff;
    }
    let frac_len = i32::try_from(frac_part.len()).ok()?;
    Some((coeff, exp_part - frac_len))
}

fn decimal_remainder(self_val: f64, divisor: f64) -> f64 {
    if !self_val.is_finite() || !divisor.is_finite() || divisor == 0.0 {
        return f64::NAN;
    }
    if self_val.fract() == 0.0 && divisor.fract() == 0.0 {
        return self_val % divisor;
    }
    let Some((sc, se)) = to_scientific_int(self_val) else {
        return f64::NAN;
    };
    let Some((dc, de)) = to_scientific_int(divisor) else {
        return f64::NAN;
    };
    let exp = se.min(de);
    let ten = BigInt::from(10u32);
    let si = sc * ten.pow(u32::try_from(se - exp).unwrap_or(0));
    let di = dc * ten.pow(u32::try_from(de - exp).unwrap_or(0));
    let out = si % di;
    if out.is_zero() {
        return if self_val < 0.0 || self_val.is_sign_negative() {
            -0.0
        } else {
            0.0
        };
    }
    let parsed = format!("{out}e{exp}").parse::<f64>().unwrap_or(0.0);
    if parsed == 0.0 {
        // JS Number.MIN_VALUE is the smallest positive subnormal (5e-324 == f64::from_bits(1)).
        let min_subnormal = f64::from_bits(1);
        if self_val.is_sign_negative() {
            -min_subnormal
        } else {
            min_subnormal
        }
    } else {
        parsed
    }
}
