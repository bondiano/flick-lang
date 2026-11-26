//! Counted `BigDecimal` primitive implementations (`packages/std/src/BigDecimal.fl`).

use std::cmp::Ordering;

use flick_core::ir::Lit;
use flick_core::ir::memory::BigDecimalIntrinsic;
use flick_core::opt::{js_number, json_string};
use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use super::bigint_ops::parse_operand;
use super::decimal_round::{
    Dec, MAX_DECIMAL_SCALE, dec_ceil, dec_compare, dec_floor, dec_round, dec_sum, dec_truncate,
    default_divide, pow10, round_quotient,
};
use super::list_ops::slice_reps;
use super::string_ops::js_trim;
use super::{Data, Error, Fuel, Heap, Value, charge};

const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

#[expect(clippy::float_cmp, reason = "exact safe-integer check")]
fn safe_scale(f: f64) -> Option<i64> {
    if f.is_finite() && f == f.trunc() && f.abs() <= MAX_SAFE {
        #[expect(clippy::cast_possible_truncation, reason = "bounded by 2^53 - 1")]
        Some(f as i64)
    } else {
        None
    }
}

fn expect_float(val: &Value) -> Result<f64, Error> {
    match *val {
        Value::Float(f) => Ok(f),
        Value::Int(i) => Ok(f64::from(i)),
        _ => Err(Error::ExpectedFloat),
    }
}

fn expect_dec(heap: &Heap, val: &Value) -> Result<Dec, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    let Data::BigDecimal { value, scale } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    Ok(Dec {
        value: value.clone(),
        scale: *scale,
    })
}

fn alloc_dec(heap: &mut Heap, d: Dec) -> Value {
    heap.allocate(Data::BigDecimal {
        value: d.value,
        scale: d.scale,
    })
}

fn decimal_err(heap: &mut Heap, message: String) -> Result<Value, Error> {
    let s = heap.literal(&Lit::String(message))?;
    let err = heap.construct("BigDecimal.DecimalError.DecimalError", vec![s])?;
    heap.construct("Prelude.Result.Err", vec![err])
}

fn mode_name(heap: &Heap, val: &Value) -> Result<String, Error> {
    let tag = heap.tag(val)?;
    Ok(tag.rsplit('.').next().unwrap_or(tag).to_owned())
}

fn ordering_val(heap: &mut Heap, ord: Ordering) -> Result<Value, Error> {
    let name = match ord {
        Ordering::Less => "LT",
        Ordering::Equal => "EQ",
        Ordering::Greater => "GT",
    };
    heap.construct(&format!("Prelude.Ordering.{name}"), vec![])
}

pub(super) fn compare_raw(av: &BigInt, as_: i64, bv: &BigInt, bs: i64) -> Result<Ordering, Error> {
    dec_compare(
        &Dec {
            value: av.clone(),
            scale: as_,
        },
        &Dec {
            value: bv.clone(),
            scale: bs,
        },
    )
}

pub(crate) fn format_decimal(value: &BigInt, scale: i64) -> String {
    let n = Dec {
        value: value.clone(),
        scale,
    }
    .normalize();
    if n.scale >= 16 || n.scale <= -16 {
        return format_exponential(&n);
    }
    let negative = n.is_negative();
    let abs = n.value.abs().to_str_radix(10);
    let out = if n.scale > 0 {
        let s = usize::try_from(n.scale).unwrap_or(0);
        let pad = (s + 1).saturating_sub(abs.len());
        let digits = format!("{}{abs}", "0".repeat(pad));
        let point = digits.len() - s;
        format!("{}.{}", &digits[..point], &digits[point..])
    } else {
        let zeros = usize::try_from(-n.scale).unwrap_or(0);
        format!("{abs}{}", "0".repeat(zeros))
    };
    if negative { format!("-{out}") } else { out }
}

fn format_exponential(d: &Dec) -> String {
    if d.is_zero() {
        return "0e+0".into();
    }
    let n = d.normalize();
    let digits = n.value.to_str_radix(10);
    let point = if n.is_negative() { 2 } else { 1 };
    let (head, tail) = digits.split_at(point);
    let exp = i64::try_from(tail.len()).unwrap_or(0) - n.scale;
    let mantissa = if tail.is_empty() {
        head.to_owned()
    } else {
        format!("{head}.{tail}")
    };
    if exp >= 0 {
        format!("{mantissa}e+{exp}")
    } else {
        format!("{mantissa}e{exp}")
    }
}

pub(super) fn run(
    heap: &mut Heap,
    op: BigDecimalIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        BigDecimalIntrinsic::FromString => from_string(heap, &args[0], fuel),
        BigDecimalIntrinsic::FromBigInt => from_bigint(heap, &args[0], &args[1], fuel),
        BigDecimalIntrinsic::FromInteger => {
            let v = parse_operand(heap, &args[0], fuel)?;
            Ok(alloc_dec(heap, Dec { value: v, scale: 0 }))
        }
        BigDecimalIntrinsic::FromNumber => from_number(heap, &args[0]),
        BigDecimalIntrinsic::ToString | BigDecimalIntrinsic::ToStringNormal => {
            let d = expect_dec(heap, &args[0])?;
            heap.literal(&Lit::String(format_decimal(&d.value, d.scale)))
        }
        BigDecimalIntrinsic::ToExponential => {
            let d = expect_dec(heap, &args[0])?;
            heap.literal(&Lit::String(format_exponential(&d)))
        }
        BigDecimalIntrinsic::Scale => {
            let d = expect_dec(heap, &args[0])?;
            #[expect(clippy::cast_precision_loss, reason = "scale is bounded safe int")]
            Ok(Value::Float(d.scale as f64))
        }
        BigDecimalIntrinsic::Add => {
            let a = expect_dec(heap, &args[0])?;
            let b = expect_dec(heap, &args[1])?;
            Ok(alloc_dec(heap, dec_sum(&a, &b)?))
        }
        BigDecimalIntrinsic::Subtract => {
            let a = expect_dec(heap, &args[0])?;
            let b = expect_dec(heap, &args[1])?;
            Ok(alloc_dec(
                heap,
                dec_sum(
                    &a,
                    &Dec {
                        value: -b.value,
                        scale: b.scale,
                    },
                )?,
            ))
        }
        BigDecimalIntrinsic::Multiply => multiply(heap, &args[0], &args[1]),
        BigDecimalIntrinsic::DivideToScale => {
            divide_to_scale(heap, &args[0], &args[1], &args[2], &args[3])
        }
        BigDecimalIntrinsic::Normalize => {
            Ok(alloc_dec(heap, expect_dec(heap, &args[0])?.normalize()))
        }
        BigDecimalIntrinsic::SetScale => apply_scaled(heap, &args[0], &args[1], Dec::rescale),
        BigDecimalIntrinsic::Truncate => apply_scaled(heap, &args[0], &args[1], dec_truncate),
        BigDecimalIntrinsic::Ceil => apply_scaled(heap, &args[0], &args[1], dec_ceil),
        BigDecimalIntrinsic::Floor => apply_scaled(heap, &args[0], &args[1], dec_floor),
        BigDecimalIntrinsic::Round => round_op(heap, &args[0], &args[1], &args[2]),
        BigDecimalIntrinsic::SumAll => sum_all(heap, &args[0], fuel),
        BigDecimalIntrinsic::MultiplyAll => multiply_all(heap, &args[0], fuel),
        BigDecimalIntrinsic::Divide => divide_op(heap, &args[0], &args[1]),
        BigDecimalIntrinsic::Remainder => remainder_op(heap, &args[0], &args[1]),
        BigDecimalIntrinsic::Compare => {
            let a = expect_dec(heap, &args[0])?;
            let b = expect_dec(heap, &args[1])?;
            ordering_val(heap, dec_compare(&a, &b)?)
        }
        BigDecimalIntrinsic::Sign => sign_op(heap, &args[0]),
        BigDecimalIntrinsic::Abs => {
            let d = expect_dec(heap, &args[0])?;
            Ok(alloc_dec(
                heap,
                Dec {
                    value: d.value.abs(),
                    scale: d.scale,
                },
            ))
        }
        BigDecimalIntrinsic::Negate => {
            let d = expect_dec(heap, &args[0])?;
            Ok(alloc_dec(
                heap,
                Dec {
                    value: -d.value,
                    scale: d.scale,
                },
            ))
        }
        BigDecimalIntrinsic::IsInteger => Ok(Value::Bool(
            expect_dec(heap, &args[0])?.normalize().scale <= 0,
        )),
        BigDecimalIntrinsic::IsZero => Ok(Value::Bool(expect_dec(heap, &args[0])?.is_zero())),
        BigDecimalIntrinsic::IsNegative => {
            Ok(Value::Bool(expect_dec(heap, &args[0])?.is_negative()))
        }
        BigDecimalIntrinsic::IsPositive => {
            Ok(Value::Bool(expect_dec(heap, &args[0])?.is_positive()))
        }
    }
}

fn from_bigint(
    heap: &mut Heap,
    v_val: &Value,
    s_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let v = parse_operand(heap, v_val, fuel)?;
    let sf = expect_float(s_val)?;
    match safe_scale(sf) {
        Some(s) => {
            let d = alloc_dec(heap, Dec { value: v, scale: s });
            heap.construct("Prelude.Result.Ok", vec![d])
        }
        None => decimal_err(
            heap,
            format!("scale {} is not a safe integer", js_number(sf)),
        ),
    }
}

fn divide_op(heap: &mut Heap, div_val: &Value, self_val: &Value) -> Result<Value, Error> {
    let divisor = expect_dec(heap, div_val)?;
    let dividend = expect_dec(heap, self_val)?;
    match default_divide(&dividend, &divisor) {
        Some(q) => {
            let v = alloc_dec(heap, q);
            heap.construct("Prelude.Option.Some", vec![v])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn remainder_op(heap: &mut Heap, div_val: &Value, self_val: &Value) -> Result<Value, Error> {
    let divisor = expect_dec(heap, div_val)?;
    let dividend = expect_dec(heap, self_val)?;
    if divisor.is_zero() {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let s = dividend.scale.max(divisor.scale);
    let rem = dividend.rescale(s)?.value % divisor.rescale(s)?.value;
    let v = alloc_dec(
        heap,
        Dec {
            value: rem,
            scale: s,
        },
    );
    heap.construct("Prelude.Option.Some", vec![v])
}

fn sign_op(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let d = expect_dec(heap, val)?;
    let ord = if d.is_negative() {
        Ordering::Less
    } else if d.is_positive() {
        Ordering::Greater
    } else {
        Ordering::Equal
    };
    ordering_val(heap, ord)
}

fn is_signed_digits(s: &str) -> bool {
    let digits = s
        .strip_prefix('+')
        .or_else(|| s.strip_prefix('-'))
        .unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

pub(super) fn parse_decimal(s: &str) -> Option<Dec> {
    if s.is_empty() {
        return Some(Dec::zero());
    }
    let (base, exp) = if let Some(idx) = s.find(['e', 'E']) {
        let base = &s[..idx];
        let trail = &s[idx + 1..];
        if base.is_empty() || !is_signed_digits(trail) {
            return None;
        }
        let e = trail.parse::<i64>().ok()?;
        if e.unsigned_abs() > 9_007_199_254_740_991 {
            return None;
        }
        (base, e)
    } else {
        (s, 0_i64)
    };
    let (digits, offset) = if let Some(dot) = base.find('.') {
        let trail = &base[dot + 1..];
        if !trail.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        (
            format!("{}{trail}", &base[..dot]),
            i64::try_from(trail.len()).ok()?,
        )
    } else {
        (base.to_owned(), 0_i64)
    };
    if !is_signed_digits(&digits) {
        return None;
    }
    let scale = offset.checked_sub(exp)?;
    let cleaned = digits.strip_prefix('+').unwrap_or(&digits);
    let v = BigInt::parse_bytes(cleaned.as_bytes(), 10)?;
    let mut normalized = scale;
    if !v.is_zero() {
        let trailing = digits.len() - digits.trim_end_matches('0').len();
        normalized -= i64::try_from(trailing).unwrap_or(0);
    }
    if scale > MAX_DECIMAL_SCALE || normalized < -MAX_DECIMAL_SCALE {
        return None;
    }
    Some(Dec { value: v, scale })
}

fn from_string(heap: &mut Heap, val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let text = heap.string_lossy(val)?;
    charge(fuel, text.len())?;
    match parse_decimal(js_trim(&text)) {
        Some(d) => {
            let dv = alloc_dec(heap, d);
            heap.construct("Prelude.Result.Ok", vec![dv])
        }
        None => decimal_err(heap, format!("not a decimal: {}", json_string(&text))),
    }
}

fn from_number(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let x = expect_float(val)?;
    if !x.is_finite() {
        return decimal_err(heap, format!("{} is not finite", js_number(x)));
    }
    let raw = js_number(x);
    let text = if raw == "-0" { "0" } else { raw.as_str() };
    if text.contains('e') {
        return match parse_decimal(text) {
            Some(d) => {
                let dv = alloc_dec(heap, d);
                heap.construct("Prelude.Result.Ok", vec![dv])
            }
            None => decimal_err(heap, format!("{text} has no decimal form")),
        };
    }
    let (lead, trail) = text.split_once('.').unwrap_or((text, ""));
    let combined = format!("{lead}{trail}");
    let v = BigInt::parse_bytes(combined.as_bytes(), 10).unwrap_or_else(BigInt::zero);
    let dv = alloc_dec(
        heap,
        Dec {
            value: v,
            scale: i64::try_from(trail.len()).unwrap_or(0),
        },
    );
    heap.construct("Prelude.Result.Ok", vec![dv])
}

fn multiply(heap: &mut Heap, a_val: &Value, b_val: &Value) -> Result<Value, Error> {
    let x = expect_dec(heap, a_val)?;
    let y = expect_dec(heap, b_val)?;
    let sum_scale = x.scale.saturating_add(y.scale);
    if sum_scale.unsigned_abs() > 9_007_199_254_740_991 {
        return decimal_err(
            heap,
            format!("the product's scale {sum_scale} is not a safe integer"),
        );
    }
    let out = if x.is_zero() || y.is_zero() {
        Dec::zero()
    } else {
        Dec {
            value: x.value * y.value,
            scale: sum_scale,
        }
    };
    let dv = alloc_dec(heap, out);
    heap.construct("Prelude.Result.Ok", vec![dv])
}

fn divide_to_scale(
    heap: &mut Heap,
    scale_val: &Value,
    mode_val: &Value,
    a_val: &Value,
    b_val: &Value,
) -> Result<Value, Error> {
    let sf = expect_float(scale_val)?;
    let Some(s) = safe_scale(sf) else {
        return decimal_err(
            heap,
            format!("scale {} is not a safe integer", js_number(sf)),
        );
    };
    let x = expect_dec(heap, a_val)?;
    let y = expect_dec(heap, b_val)?;
    if y.is_zero() {
        return decimal_err(heap, "division by zero".into());
    }
    let k = y.scale + s - x.scale;
    let (num, den) = if k >= 0 {
        (&x.value * pow10(k), y.value)
    } else {
        (x.value, &y.value * pow10(-k))
    };
    let mode = mode_name(heap, mode_val)?;
    match round_quotient(&num, &den, &mode) {
        Some(q) => {
            let dv = alloc_dec(heap, Dec { value: q, scale: s });
            heap.construct("Prelude.Result.Ok", vec![dv])
        }
        None => decimal_err(
            heap,
            "the quotient needs rounding, and the mode is Exact".into(),
        ),
    }
}

fn apply_scaled(
    heap: &mut Heap,
    scale_val: &Value,
    dec_val: &Value,
    f: impl FnOnce(&Dec, i64) -> Result<Dec, Error>,
) -> Result<Value, Error> {
    let sf = expect_float(scale_val)?;
    let Some(s) = safe_scale(sf) else {
        return decimal_err(
            heap,
            format!("scale {} is not a safe integer", js_number(sf)),
        );
    };
    let d = expect_dec(heap, dec_val)?;
    let dv = alloc_dec(heap, f(&d, s)?);
    heap.construct("Prelude.Result.Ok", vec![dv])
}

fn round_op(
    heap: &mut Heap,
    scale_val: &Value,
    mode_val: &Value,
    dec_val: &Value,
) -> Result<Value, Error> {
    let sf = expect_float(scale_val)?;
    let Some(s) = safe_scale(sf) else {
        return decimal_err(
            heap,
            format!("scale {} is not a safe integer", js_number(sf)),
        );
    };
    let d = expect_dec(heap, dec_val)?;
    let mode = mode_name(heap, mode_val)?;
    if mode != "Exact" {
        let dv = alloc_dec(heap, dec_round(&d, s, &mode)?);
        return heap.construct("Prelude.Result.Ok", vec![dv]);
    }
    let cut = dec_truncate(&d, s)?;
    if dec_compare(&cut, &d)? == Ordering::Equal {
        let dv = alloc_dec(heap, cut);
        heap.construct("Prelude.Result.Ok", vec![dv])
    } else {
        decimal_err(heap, "rounding is needed, and the mode is Exact".into())
    }
}

fn sum_all(heap: &mut Heap, list_val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut acc = Dec::zero();
    for rep in &reps {
        let d = expect_dec(heap, rep)?;
        acc = dec_sum(&acc, &d)?;
    }
    Ok(alloc_dec(heap, acc))
}

fn multiply_all(heap: &mut Heap, list_val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut acc = Dec {
        value: BigInt::from(1),
        scale: 0,
    };
    for rep in &reps {
        let d = expect_dec(heap, rep)?;
        if d.is_zero() {
            return Ok(alloc_dec(heap, Dec::zero()));
        }
        acc = Dec {
            value: acc.value * d.value,
            scale: acc.scale + d.scale,
        };
    }
    Ok(alloc_dec(heap, acc))
}
