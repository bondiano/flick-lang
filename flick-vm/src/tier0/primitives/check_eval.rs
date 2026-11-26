//! Counted built-in `Check.*` evaluator (`SchemaIntrinsic::RunCheck`).

use std::cmp::Ordering;

use flick_core::ir::Lit;
use flick_core::opt::js_number;
use num_bigint::BigInt;
use num_traits::Zero;
use regex_automata::meta::Regex;

use super::check_ops::{MAX_SAFE, number_of, pair_fields, rec_field};
use super::decimal_ops::compare_raw as dec_compare;
use super::list_ops::slice_reps;
use super::schema_ops::describe_input;
use super::string_ops::js_trim;
use super::{Data, Error, Heap, Value, charge, equal};

fn length_of(heap: &Heap, val: &Value) -> Result<usize, Error> {
    if let Value::Object(tok) = val
        && matches!(heap.object(tok.id)?.data, Data::String(_) | Data::Utf16(_))
    {
        return Ok(heap.utf16_units(val)?.len());
    }
    Ok(slice_reps(heap, val)?.len())
}

fn code_points_of(heap: &Heap, val: &Value) -> Result<usize, Error> {
    let units = heap.utf16_units(val)?;
    let mut count = 0_usize;
    let mut i = 0;
    while i < units.len() {
        if (0xD800..=0xDBFF).contains(&units[i])
            && i + 1 < units.len()
            && (0xDC00..=0xDFFF).contains(&units[i + 1])
        {
            i += 2;
        } else {
            i += 1;
        }
        count += 1;
    }
    Ok(count)
}

fn number_remainder(divisor: f64, self_n: f64) -> f64 {
    let rem = self_n % divisor;
    if rem == 0.0 {
        return 0.0;
    }
    let s1 = js_number(self_n);
    let s2 = js_number(divisor);
    let dec_len = |s: &str| {
        if s.contains(['e', 'E']) {
            0
        } else {
            s.split_once('.').map_or(0, |(_, f)| f.len())
        }
    };
    let precision = dec_len(&s1).max(dec_len(&s2));
    if precision > 0 && precision <= 12 {
        let factor = 10_f64.powi(i32::try_from(precision).unwrap_or(0));
        ((self_n * factor).round() % (divisor * factor).round()) / factor
    } else {
        rem
    }
}

fn unique_values(heap: &Heap, reps: &[Value]) -> Result<bool, Error> {
    for i in 0..reps.len() {
        for j in (i + 1)..reps.len() {
            if equal(heap, &reps[i], &reps[j])? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn cmp_dec_vals(heap: &Heap, a: &Value, b: &Value) -> Result<Ordering, Error> {
    let (Value::Object(ta), Value::Object(tb)) = (a, b) else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    let (
        Data::BigDecimal {
            value: av,
            scale: as_,
        },
        Data::BigDecimal {
            value: bv,
            scale: bs,
        },
    ) = (&heap.object(ta.id)?.data, &heap.object(tb.id)?.data)
    else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    dec_compare(av, *as_, bv, *bs)
}

#[expect(clippy::too_many_lines, reason = "evaluates all built-in Check specs")]
pub(super) fn run_check(
    heap: &mut Heap,
    spec_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let Value::Object(tok) = spec_val else {
        return Err(Error::PatternKind("Check.$Spec"));
    };
    let (kind, param, desc) = match &heap.object(tok.id)?.data {
        Data::Tagged { fields, .. } if fields.len() == 3 => {
            let k = heap.string_lossy(&fields[0])?;
            let p = fields[1].representation();
            let d = match &fields[2] {
                Value::Object(mtok) => match &heap.object(mtok.id)?.data {
                    Data::Tagged { fields: mf, .. } if !mf.is_empty() => {
                        heap.string_lossy(&mf[0])?
                    }
                    _ => "a check".into(),
                },
                _ => "a check".into(),
            };
            (k, p, d)
        }
        _ => return Err(Error::PatternKind("Check.$Spec")),
    };
    charge(fuel, 1)?;
    let pass = match kind.as_str() {
        "min_length" => {
            let Value::Int(n) = param else {
                return Err(Error::ExpectedInt);
            };
            length_of(heap, val)? >= usize::try_from(n).unwrap_or(0)
        }
        "max_length" => {
            let Value::Int(n) = param else {
                return Err(Error::ExpectedInt);
            };
            length_of(heap, val)? <= usize::try_from(n).unwrap_or(0)
        }
        "length_between" => {
            let (Value::Int(lo), Value::Int(hi)) = pair_fields(heap, &param)? else {
                return Err(Error::ExpectedInt);
            };
            let n = length_of(heap, val)?;
            n >= usize::try_from(lo).unwrap_or(0) && n <= usize::try_from(hi).unwrap_or(0)
        }
        "min_code_points" => {
            let Value::Int(n) = param else {
                return Err(Error::ExpectedInt);
            };
            code_points_of(heap, val)? >= usize::try_from(n).unwrap_or(0)
        }
        "max_code_points" => {
            let Value::Int(n) = param else {
                return Err(Error::ExpectedInt);
            };
            code_points_of(heap, val)? <= usize::try_from(n).unwrap_or(0)
        }
        "code_points_between" => {
            let (Value::Int(lo), Value::Int(hi)) = pair_fields(heap, &param)? else {
                return Err(Error::ExpectedInt);
            };
            let n = code_points_of(heap, val)?;
            n >= usize::try_from(lo).unwrap_or(0) && n <= usize::try_from(hi).unwrap_or(0)
        }
        "pattern" => {
            let pat = heap.string_lossy(&param)?;
            let s = heap.string_lossy(val)?;
            charge(fuel, s.len())?;
            Regex::new(&pat).is_ok_and(|re| re.is_match(&s))
        }
        "regex_exact" => {
            let (pat_v, _) = pair_fields(heap, &param)?;
            let pat = heap.string_lossy(&pat_v)?;
            let s = heap.string_lossy(val)?;
            charge(fuel, s.len())?;
            Regex::new(&pat).is_ok_and(|re| re.is_match(&s))
        }
        "starts_with" => heap
            .string_lossy(val)?
            .starts_with(&heap.string_lossy(&param)?),
        "ends_with" => heap
            .string_lossy(val)?
            .ends_with(&heap.string_lossy(&param)?),
        "includes" => heap
            .string_lossy(val)?
            .contains(&heap.string_lossy(&param)?),
        "trimmed" => {
            let s = heap.string_lossy(val)?;
            js_trim(&s) == s
        }
        "lowercased" => {
            let s = heap.string_lossy(val)?;
            s.to_lowercase() == s
        }
        "uppercased" => {
            let s = heap.string_lossy(val)?;
            s.to_uppercase() == s
        }
        "capitalized" => {
            let units = heap.utf16_units(val)?;
            units.is_empty() || {
                let first = String::from_utf16_lossy(&units[..1]);
                first.to_uppercase() == first
            }
        }
        "uncapitalized" => {
            let units = heap.utf16_units(val)?;
            units.is_empty() || {
                let first = String::from_utf16_lossy(&units[..1]);
                first.to_lowercase() == first
            }
        }
        "gt" | "gte" | "lt" | "lte" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            let b = number_of(&param).ok_or(Error::ExpectedFloat)?;
            match kind.as_str() {
                "gt" => x > b,
                "gte" => x >= b,
                "lt" => x < b,
                _ => x <= b,
            }
        }
        "between" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            let lo = number_of(&rec_field(heap, &param, "minimum")?).ok_or(Error::ExpectedFloat)?;
            let hi = number_of(&rec_field(heap, &param, "maximum")?).ok_or(Error::ExpectedFloat)?;
            let ex_lo = matches!(
                rec_field(heap, &param, "exclusiveMinimum")?,
                Value::Bool(true)
            );
            let ex_hi = matches!(
                rec_field(heap, &param, "exclusiveMaximum")?,
                Value::Bool(true)
            );
            let ok_lo = if ex_lo { x > lo } else { x >= lo };
            let ok_hi = if ex_hi { x < hi } else { x <= hi };
            ok_lo && ok_hi
        }
        "multiple_of" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            let d = number_of(&param).ok_or(Error::ExpectedFloat)?;
            number_remainder(d, x) == 0.0
        }
        "int" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            #[expect(clippy::float_cmp, reason = "exact safe-integer check")]
            let ok = x.is_finite() && x == x.trunc() && x.abs() <= MAX_SAFE;
            ok
        }
        "finite" => number_of(val).is_some_and(f64::is_finite),
        "gt_bigint" | "gte_bigint" | "lt_bigint" | "lte_bigint" => {
            let a =
                BigInt::parse_bytes(heap.bigint(val)?.as_bytes(), 10).unwrap_or_else(BigInt::zero);
            let b = BigInt::parse_bytes(heap.bigint(&param)?.as_bytes(), 10)
                .unwrap_or_else(BigInt::zero);
            match kind.as_str() {
                "gt_bigint" => a > b,
                "gte_bigint" => a >= b,
                "lt_bigint" => a < b,
                _ => a <= b,
            }
        }
        "between_bigint" => {
            let x =
                BigInt::parse_bytes(heap.bigint(val)?.as_bytes(), 10).unwrap_or_else(BigInt::zero);
            let lo = BigInt::parse_bytes(
                heap.bigint(&rec_field(heap, &param, "minimum")?)?
                    .as_bytes(),
                10,
            )
            .unwrap_or_else(BigInt::zero);
            let hi = BigInt::parse_bytes(
                heap.bigint(&rec_field(heap, &param, "maximum")?)?
                    .as_bytes(),
                10,
            )
            .unwrap_or_else(BigInt::zero);
            let ex_lo = matches!(
                rec_field(heap, &param, "exclusiveMinimum")?,
                Value::Bool(true)
            );
            let ex_hi = matches!(
                rec_field(heap, &param, "exclusiveMaximum")?,
                Value::Bool(true)
            );
            (x > lo || (!ex_lo && x == lo)) && (x < hi || (!ex_hi && x == hi))
        }
        "gt_decimal" | "gte_decimal" | "lt_decimal" | "lte_decimal" => {
            let ord = cmp_dec_vals(heap, val, &param)?;
            match kind.as_str() {
                "gt_decimal" => ord == Ordering::Greater,
                "gte_decimal" => ord != Ordering::Less,
                "lt_decimal" => ord == Ordering::Less,
                _ => ord != Ordering::Greater,
            }
        }
        "between_decimal" => {
            let lo_ord = cmp_dec_vals(heap, val, &rec_field(heap, &param, "minimum")?)?;
            let hi_ord = cmp_dec_vals(heap, val, &rec_field(heap, &param, "maximum")?)?;
            let ex_lo = matches!(
                rec_field(heap, &param, "exclusiveMinimum")?,
                Value::Bool(true)
            );
            let ex_hi = matches!(
                rec_field(heap, &param, "exclusiveMaximum")?,
                Value::Bool(true)
            );
            let ok_lo = lo_ord == Ordering::Greater || (!ex_lo && lo_ord == Ordering::Equal);
            let ok_hi = hi_ord == Ordering::Less || (!ex_hi && hi_ord == Ordering::Equal);
            ok_lo && ok_hi
        }
        "unique" => {
            let reps = slice_reps(heap, val)?;
            charge(fuel, reps.len())?;
            unique_values(heap, &reps)?
        }
        "unique_keys" => {
            let reps = slice_reps(heap, val)?;
            charge(fuel, reps.len())?;
            let mut keys = Vec::with_capacity(reps.len());
            for r in &reps {
                let (k, _) = pair_fields(heap, r)?;
                keys.push(k);
            }
            unique_values(heap, &keys)?
        }
        _ => return Err(Error::RangeError(format!("unknown Check spec: {kind}"))),
    };
    if pass {
        heap.construct("Prelude.Option.None", vec![])
    } else {
        let got = describe_input(heap, val)?;
        let msg = heap.literal(&Lit::String(format!("expected {desc}, got {got}")))?;
        heap.construct("Prelude.Option.Some", vec![msg])
    }
}
