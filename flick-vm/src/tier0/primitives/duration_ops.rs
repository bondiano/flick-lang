//! Counted `Duration` primitive implementations (`packages/std/src/Duration.fl`).

use std::cmp::Ordering;

use flick_core::ir::Lit;
use flick_core::ir::memory::DurationIntrinsic;
use flick_core::opt::{js_number, json_string};
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};

use super::bigint_ops::parse_operand;
use super::decimal_round::pow10;
use super::{Data, DurationData, Error, Heap, Value, charge};

const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

fn expect_float(val: &Value) -> Result<f64, Error> {
    match *val {
        Value::Float(f) => Ok(f),
        Value::Int(i) => Ok(f64::from(i)),
        _ => Err(Error::ExpectedFloat),
    }
}

fn expect_dur(heap: &Heap, val: &Value) -> Result<DurationData, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Duration"));
    };
    let Data::Duration(d) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Duration"));
    };
    Ok(d.clone())
}

fn alloc_dur(heap: &mut Heap, d: DurationData) -> Value {
    heap.allocate(Data::Duration(d))
}

fn dur_err(heap: &mut Heap, message: String) -> Result<Value, Error> {
    let s = heap.literal(&Lit::String(message))?;
    let err = heap.construct("Duration.DurationError.DurationError", vec![s])?;
    heap.construct("Prelude.Result.Err", vec![err])
}

fn exact_int_f64(n: f64) -> BigInt {
    if n == 0.0 {
        return BigInt::zero();
    }
    let bits = n.to_bits();
    let neg = (bits >> 63) != 0;
    let raw_exp = i32::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let exp = raw_exp - 1023 - 52;
    let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let mut big = BigInt::from(mantissa);
    if exp >= 0 {
        big <<= usize::try_from(exp).unwrap_or(0);
    } else {
        big >>= usize::try_from(-exp).unwrap_or(0);
    }
    if neg { -big } else { big }
}

fn round_away(x: f64) -> BigInt {
    let r = x.abs().round().copysign(x);
    exact_int_f64(r)
}

#[expect(clippy::float_cmp, reason = "exact JS duration truncation check")]
pub(super) fn from_millis(ms: f64) -> DurationData {
    if ms.is_nan() || ms == 0.0 {
        DurationData::Millis(0.0)
    } else if ms.is_infinite() {
        if ms.is_sign_positive() {
            DurationData::Infinite
        } else {
            DurationData::NegativeInfinite
        }
    } else if ms != ms.trunc() {
        DurationData::Nanos(round_away(ms * 1e6))
    } else {
        DurationData::Millis(ms)
    }
}

pub(super) fn from_nanos(n: BigInt) -> DurationData {
    if n.is_zero() {
        DurationData::Millis(0.0)
    } else {
        DurationData::Nanos(n)
    }
}

pub(super) fn is_infinite(d: &DurationData) -> bool {
    matches!(d, DurationData::Infinite | DurationData::NegativeInfinite)
}

pub(super) fn is_negative(d: &DurationData) -> bool {
    match d {
        DurationData::Millis(ms) => *ms < 0.0,
        DurationData::Nanos(n) => n.is_negative(),
        DurationData::Infinite => false,
        DurationData::NegativeInfinite => true,
    }
}

fn is_zero(d: &DurationData) -> bool {
    match d {
        DurationData::Millis(ms) => *ms == 0.0,
        DurationData::Nanos(n) => n.is_zero(),
        DurationData::Infinite | DurationData::NegativeInfinite => false,
    }
}

#[expect(clippy::float_cmp, reason = "exact integral millisecond check")]
pub(super) fn to_nanos_big(d: &DurationData) -> BigInt {
    match d {
        DurationData::Nanos(n) => n.clone(),
        DurationData::Millis(ms) => {
            if *ms == ms.trunc() {
                exact_int_f64(*ms) * BigInt::from(1_000_000)
            } else {
                round_away(*ms * 1e6)
            }
        }
        DurationData::Infinite | DurationData::NegativeInfinite => BigInt::zero(),
    }
}

fn non_negative(heap: &mut Heap, what: &str, d: DurationData) -> Result<Value, Error> {
    if is_negative(&d) {
        dur_err(heap, format!("{what}: the result is negative"))
    } else {
        let v = alloc_dur(heap, d);
        heap.construct("Prelude.Result.Ok", vec![v])
    }
}

fn checked_unit(
    heap: &mut Heap,
    unit: &str,
    n_val: &Value,
    millis_per: f64,
) -> Result<Value, Error> {
    let n = expect_float(n_val)?;
    if n.is_nan() {
        return dur_err(heap, format!("{unit}: NaN is not a duration"));
    }
    if n < 0.0 {
        return dur_err(heap, format!("{unit}: {} is negative", js_number(n)));
    }
    let ms = n * millis_per;
    if ms.is_infinite() {
        return dur_err(
            heap,
            format!(
                "{unit}: {} overflows; use Duration.infinity for an unbounded duration",
                js_number(n)
            ),
        );
    }
    let v = alloc_dur(heap, from_millis(ms));
    heap.construct("Prelude.Result.Ok", vec![v])
}

pub(super) fn compare_raw(a: &DurationData, b: &DurationData) -> Ordering {
    if is_infinite(a) || is_infinite(b) {
        return match (a, b) {
            (DurationData::Infinite, DurationData::Infinite)
            | (DurationData::NegativeInfinite, DurationData::NegativeInfinite) => Ordering::Equal,
            (DurationData::Infinite, _) | (_, DurationData::NegativeInfinite) => Ordering::Greater,
            _ => Ordering::Less,
        };
    }
    if let (DurationData::Millis(x), DurationData::Millis(y)) = (a, b) {
        return x.partial_cmp(y).unwrap_or(Ordering::Equal);
    }
    to_nanos_big(a).cmp(&to_nanos_big(b))
}

fn saturate_i32(n: &BigInt) -> i32 {
    n.to_i64()
        .map_or(i32::MAX, |v| i32::try_from(v).unwrap_or(i32::MAX))
}

fn parts_tuple(d: &DurationData) -> (i32, i32, i32, i32, i32, i32) {
    if is_infinite(d) {
        return (i32::MAX, i32::MAX, i32::MAX, i32::MAX, i32::MAX, i32::MAX);
    }
    let n = to_nanos_big(d).abs();
    let part = |per: i64, modulus: i64| -> i32 {
        let mut q = &n / BigInt::from(per);
        if modulus > 0 {
            q %= BigInt::from(modulus);
        }
        saturate_i32(&q)
    };
    (
        part(86_400_000_000_000, 0),
        part(3_600_000_000_000, 24),
        part(60_000_000_000, 60),
        part(1_000_000_000, 60),
        part(1_000_000, 1000),
        part(1, 1_000_000),
    )
}

pub(super) fn format_duration(d: &DurationData) -> String {
    match d {
        DurationData::NegativeInfinite => return "-Infinity".into(),
        DurationData::Infinite => return "Infinity".into(),
        _ if is_zero(d) => return "0".into(),
        _ => {}
    }
    let (_, hours, minutes, seconds, millis, nanos) = parts_tuple(d);
    let mut pieces = Vec::new();
    let days_big = to_nanos_big(d).abs() / BigInt::from(86_400_000_000_000_i64);
    let days = days_big.to_f64().unwrap_or(f64::INFINITY);
    if days != 0.0 {
        pieces.push(format!("{}d", js_number(days)));
    }
    for (val, suffix) in [
        (hours, "h"),
        (minutes, "m"),
        (seconds, "s"),
        (millis, "ms"),
        (nanos, "ns"),
    ] {
        if val != 0 {
            pieces.push(format!("{val}{suffix}"));
        }
    }
    let out = pieces.join(" ");
    if is_negative(d) {
        format!("-{out}")
    } else {
        out
    }
}

pub(super) fn run(
    heap: &mut Heap,
    op: DurationIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        DurationIntrinsic::Zero => Ok(alloc_dur(heap, DurationData::Millis(0.0))),
        DurationIntrinsic::Infinity => Ok(alloc_dur(heap, DurationData::Infinite)),
        DurationIntrinsic::Nanos => from_nanos_checked(heap, &args[0], 1, "nanos", fuel),
        DurationIntrinsic::Micros => from_nanos_checked(heap, &args[0], 1000, "micros", fuel),
        DurationIntrinsic::Millis => checked_unit(heap, "millis", &args[0], 1.0),
        DurationIntrinsic::Seconds => checked_unit(heap, "seconds", &args[0], 1_000.0),
        DurationIntrinsic::Minutes => checked_unit(heap, "minutes", &args[0], 60_000.0),
        DurationIntrinsic::Hours => checked_unit(heap, "hours", &args[0], 3_600_000.0),
        DurationIntrinsic::Days => checked_unit(heap, "days", &args[0], 86_400_000.0),
        DurationIntrinsic::Weeks => checked_unit(heap, "weeks", &args[0], 604_800_000.0),
        DurationIntrinsic::FromInput => from_input(heap, &args[0], fuel),
        DurationIntrinsic::IsFinite => Ok(Value::Bool(!is_infinite(&expect_dur(heap, &args[0])?))),
        DurationIntrinsic::IsZero => Ok(Value::Bool(is_zero(&expect_dur(heap, &args[0])?))),
        DurationIntrinsic::IsPositive => {
            let d = expect_dur(heap, &args[0])?;
            Ok(Value::Bool(
                matches!(d, DurationData::Infinite)
                    || (!is_infinite(&d) && !is_negative(&d) && !is_zero(&d)),
            ))
        }
        DurationIntrinsic::ToMillis => to_unit(heap, &args[0], 1.0),
        DurationIntrinsic::ToSeconds => to_unit(heap, &args[0], 1_000.0),
        DurationIntrinsic::ToMinutes => to_unit(heap, &args[0], 60_000.0),
        DurationIntrinsic::ToHours => to_unit(heap, &args[0], 3_600_000.0),
        DurationIntrinsic::ToDays => to_unit(heap, &args[0], 86_400_000.0),
        DurationIntrinsic::ToWeeks => to_unit(heap, &args[0], 604_800_000.0),
        DurationIntrinsic::ToNanos => to_nanos_op(heap, &args[0]),
        DurationIntrinsic::ToHrTime => to_hr_time(heap, &args[0]),
        DurationIntrinsic::Parts => parts_op(heap, &args[0]),
        DurationIntrinsic::Format => {
            let d = expect_dur(heap, &args[0])?;
            heap.literal(&Lit::String(format_duration(&d)))
        }
        DurationIntrinsic::Deconstruct => deconstruct(heap, &args[0]),
        DurationIntrinsic::Add => add_op(heap, &args[0], &args[1]),
        DurationIntrinsic::Subtract => subtract_op(heap, &args[0], &args[1]),
        DurationIntrinsic::Times => times_op(heap, &args[0], &args[1]),
        DurationIntrinsic::Divide => divide_op(heap, &args[0], &args[1]),
        DurationIntrinsic::Compare => {
            let a = expect_dur(heap, &args[0])?;
            let b = expect_dur(heap, &args[1])?;
            let name = match compare_raw(&a, &b) {
                Ordering::Less => "LT",
                Ordering::Equal => "EQ",
                Ordering::Greater => "GT",
            };
            heap.construct(&format!("Prelude.Ordering.{name}"), vec![])
        }
    }
}

fn from_nanos_checked(
    heap: &mut Heap,
    val: &Value,
    scale: i64,
    unit: &str,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = parse_operand(heap, val, fuel)?;
    if n.is_negative() {
        return dur_err(heap, format!("{unit}: {} is negative", n.to_str_radix(10)));
    }
    let d = alloc_dur(heap, from_nanos(n * BigInt::from(scale)));
    heap.construct("Prelude.Result.Ok", vec![d])
}

fn parse_nanos_decimal(text: &str, scale: i64) -> BigInt {
    let Some((whole, frac)) = text.split_once('.') else {
        let n = BigInt::parse_bytes(text.as_bytes(), 10).unwrap_or_else(BigInt::zero);
        return n * BigInt::from(scale);
    };
    let neg = whole.starts_with('-');
    let w = BigInt::parse_bytes(whole.trim_start_matches('-').as_bytes(), 10)
        .unwrap_or_else(BigInt::zero);
    let f = BigInt::parse_bytes(frac.as_bytes(), 10).unwrap_or_else(BigInt::zero);
    let den = pow10(i64::try_from(frac.len()).unwrap_or(0));
    let num = (w * &den + f) * BigInt::from(scale);
    let mut q = &num / &den;
    let r = &num % &den;
    if r * BigInt::from(2) >= den {
        q += 1;
    }
    if neg { -q } else { q }
}

pub(super) fn parse_duration_text(text: &str) -> Option<DurationData> {
    if text == "Infinity" {
        return Some(DurationData::Infinite);
    }
    if !text.contains(' ') {
        return None;
    }
    let mut parts = text.split_ascii_whitespace();
    let num_str = parts.next()?;
    let unit_raw = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let unsigned = num_str.strip_prefix('-').unwrap_or(num_str);
    let valid_num = if let Some((a, b)) = unsigned.split_once('.') {
        !a.is_empty()
            && a.bytes().all(|c| c.is_ascii_digit())
            && !b.is_empty()
            && b.bytes().all(|c| c.is_ascii_digit())
    } else {
        !unsigned.is_empty() && unsigned.bytes().all(|c| c.is_ascii_digit())
    };
    if !valid_num {
        return None;
    }
    // Reject leading/trailing whitespace since regex is `^(-?\d+(?:\.\d+)?)\s+(...)$`.
    if text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace) {
        return None;
    }
    let unit = unit_raw.strip_suffix('s').unwrap_or(unit_raw);
    match unit {
        "nano" => Some(from_nanos(parse_nanos_decimal(num_str, 1))),
        "micro" => Some(from_nanos(parse_nanos_decimal(num_str, 1000))),
        "milli" | "second" | "minute" | "hour" | "day" | "week" => {
            let per = match unit {
                "milli" => 1.0,
                "second" => 1_000.0,
                "minute" => 60_000.0,
                "hour" => 3_600_000.0,
                "day" => 86_400_000.0,
                _ => 604_800_000.0,
            };
            let n = num_str.parse::<f64>().ok()?;
            Some(from_millis(n * per))
        }
        _ => None,
    }
}

fn from_input(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let text = heap.string_lossy(val)?;
    charge(fuel, text.len())?;
    let quoted = json_string(&text);
    let Some(d) = parse_duration_text(&text) else {
        return dur_err(heap, format!("not a duration: {quoted}"));
    };
    non_negative(heap, &format!("from_input {quoted}"), d)
}

fn to_unit(heap: &mut Heap, val: &Value, per: f64) -> Result<Value, Error> {
    let d = expect_dur(heap, val)?;
    match d {
        DurationData::Millis(ms) => {
            heap.construct("Prelude.Option.Some", vec![Value::Float(ms / per)])
        }
        DurationData::Nanos(n) => {
            let f = n.to_f64().unwrap_or(f64::INFINITY) / (per * 1e6);
            heap.construct("Prelude.Option.Some", vec![Value::Float(f)])
        }
        DurationData::Infinite | DurationData::NegativeInfinite => {
            heap.construct("Prelude.Option.None", vec![])
        }
    }
}

fn to_nanos_op(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let d = expect_dur(heap, val)?;
    if is_infinite(&d) {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let bi = heap.literal(&Lit::BigInt(to_nanos_big(&d).to_str_radix(10)))?;
    heap.construct("Prelude.Option.Some", vec![bi])
}

fn to_hr_time(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let d = expect_dur(heap, val)?;
    if is_infinite(&d) {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let n = to_nanos_big(&d).abs();
    let billion = BigInt::from(1_000_000_000_i64);
    let s = (&n / &billion).to_f64().unwrap_or(f64::INFINITY);
    let ns = (&n % &billion).to_f64().unwrap_or(0.0);
    let sign = if is_negative(&d) { -1.0 } else { 1.0 };
    let pair = heap.construct(
        "$tuple",
        vec![Value::Float(sign * s), Value::Float(sign * ns)],
    )?;
    heap.construct("Prelude.Option.Some", vec![pair])
}

fn parts_op(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let (days, hours, minutes, seconds, millis, nanos) = parts_tuple(&expect_dur(heap, val)?);
    heap.record(vec![
        ("days".into(), Value::Int(days)),
        ("hours".into(), Value::Int(hours)),
        ("millis".into(), Value::Int(millis)),
        ("minutes".into(), Value::Int(minutes)),
        ("nanos".into(), Value::Int(nanos)),
        ("seconds".into(), Value::Int(seconds)),
    ])
}

fn deconstruct(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    match expect_dur(heap, val)? {
        DurationData::Millis(ms) => {
            heap.construct("Duration.$Decon.$Millis", vec![Value::Float(ms)])
        }
        DurationData::Nanos(n) => {
            let bi = heap.literal(&Lit::BigInt(n.to_str_radix(10)))?;
            heap.construct("Duration.$Decon.$Nanos", vec![bi])
        }
        DurationData::Infinite | DurationData::NegativeInfinite => {
            heap.construct("Duration.$Decon.$Infinity", vec![])
        }
    }
}

fn add_op(heap: &mut Heap, a_val: &Value, b_val: &Value) -> Result<Value, Error> {
    let a = expect_dur(heap, a_val)?;
    let b = expect_dur(heap, b_val)?;
    let out = if is_infinite(&a) || is_infinite(&b) {
        DurationData::Infinite
    } else if let (DurationData::Millis(x), DurationData::Millis(y)) = (&a, &b) {
        from_millis(x + y)
    } else {
        from_nanos(to_nanos_big(&a) + to_nanos_big(&b))
    };
    Ok(alloc_dur(heap, out))
}

fn subtract_op(heap: &mut Heap, that_val: &Value, self_val: &Value) -> Result<Value, Error> {
    let s = expect_dur(heap, self_val)?;
    let t = expect_dur(heap, that_val)?;
    let d = if is_infinite(&s) || is_infinite(&t) {
        match (&s, &t) {
            (DurationData::Infinite, DurationData::Infinite) => DurationData::Millis(0.0),
            (DurationData::Infinite, _) => DurationData::Infinite,
            _ => DurationData::NegativeInfinite,
        }
    } else if let (DurationData::Millis(x), DurationData::Millis(y)) = (&s, &t) {
        from_millis(x - y)
    } else {
        from_nanos(to_nanos_big(&s) - to_nanos_big(&t))
    };
    non_negative(heap, "subtract", d)
}

#[expect(clippy::float_cmp, reason = "exact JS integer factor check")]
fn times_op(heap: &mut Heap, n_val: &Value, self_val: &Value) -> Result<Value, Error> {
    let k = expect_float(n_val)?;
    let x = expect_dur(heap, self_val)?;
    if k.is_nan() || k < 0.0 {
        return dur_err(heap, format!("times: {} is negative or NaN", js_number(k)));
    }
    let d = match x {
        DurationData::Infinite | DurationData::NegativeInfinite => {
            if k == 0.0 {
                DurationData::Millis(0.0)
            } else {
                x
            }
        }
        DurationData::Millis(ms) => from_millis(ms * k),
        DurationData::Nanos(n) => {
            if k == k.trunc() && k.is_finite() {
                from_nanos(n * exact_int_f64(k))
            } else {
                let nf = n.to_f64().unwrap_or(f64::INFINITY);
                let scaled = nf * k;
                if k.is_infinite() || (nf.abs() <= MAX_SAFE && scaled.abs() <= MAX_SAFE) {
                    if scaled.is_finite() {
                        from_nanos(round_away(scaled))
                    } else {
                        from_millis(scaled)
                    }
                } else {
                    let mut num = k;
                    let mut den = BigInt::from(1);
                    while num != num.trunc() {
                        num *= 2.0;
                        den <<= 1_usize;
                    }
                    let p = n * exact_int_f64(num);
                    let mut q = p.abs() / &den;
                    let r = p.abs() % &den;
                    if (r << 1_usize) >= den {
                        q += 1;
                    }
                    if p.is_negative() {
                        q = -q;
                    }
                    from_nanos(q)
                }
            }
        }
    };
    non_negative(heap, "times", d)
}

#[expect(clippy::float_cmp, reason = "exact JS integer divisor check")]
fn divide_op(heap: &mut Heap, n_val: &Value, self_val: &Value) -> Result<Value, Error> {
    let by = expect_float(n_val)?;
    let x = expect_dur(heap, self_val)?;
    let fail = |h: &mut Heap| dur_err(h, format!("divide: cannot divide by {}", js_number(by)));
    if by <= 0.0 || !by.is_finite() {
        return fail(heap);
    }
    let d = match x {
        DurationData::Infinite | DurationData::NegativeInfinite => x,
        DurationData::Millis(ms) => from_millis(ms / by),
        DurationData::Nanos(n) => {
            if by != by.trunc() {
                return fail(heap);
            }
            from_nanos(n / exact_int_f64(by))
        }
    };
    non_negative(heap, "divide", d)
}
