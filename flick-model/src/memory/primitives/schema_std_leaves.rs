//! Counted std-type (`Duration`, `BigDecimal`, `ByteSize`, string-tree) leaf decoders.

use flick_core::ir::Lit;
use num_bigint::BigInt;
use num_traits::Zero;

use super::decimal_ops::parse_decimal;
use super::decimal_round::pow10;
use super::duration_ops::{from_millis, from_nanos, is_negative, parse_duration_text};
use super::json_ops::schema_error_single;
use super::schema_leaves::{
    MAX_SAFE, check_i32, is_digits, is_str_val, number_of, parse_tree_number,
};
use super::schema_ops::expected_err;
use super::{Data, DurationData, Error, Heap, Value};

#[expect(
    clippy::too_many_lines,
    reason = "centralized std-type leaf decoder dispatch"
)]
pub(super) fn decode_std_leaf(heap: &mut Heap, kind: &str, input: &Value) -> Result<Value, Error> {
    match kind {
        "duration_millis" => {
            let Some(x) = number_of(input).filter(|v| v.is_finite()) else {
                return expected_err(heap, "a finite number", input);
            };
            if x < 0.0 {
                return expected_err(heap, "a value greater than or equal to 0", input);
            }
            let d = heap.allocate(Data::Duration(from_millis(x)));
            heap.construct("Prelude.Result.Ok", vec![d])
        }
        "duration_nanos" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string", input);
            }
            let text = heap.string_lossy(input)?;
            if !is_digits(&text, false) {
                return expected_err(heap, "a string matching the RegExp ^[0-9]+$", input);
            }
            let n = BigInt::parse_bytes(text.as_bytes(), 10).unwrap_or_else(BigInt::zero);
            let d = heap.allocate(Data::Duration(from_nanos(n)));
            heap.construct("Prelude.Result.Ok", vec![d])
        }
        "duration_string" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string that will be decoded as a Duration", input);
            }
            let text = heap.string_lossy(input)?;
            let parsed = if text == "-Infinity" {
                Some(DurationData::NegativeInfinite)
            } else {
                parse_duration_text(&text)
            };
            let Some(d) = parsed else {
                return schema_error_single(heap, "", "an invalid value");
            };
            if is_negative(&d) {
                let dv = heap.allocate(Data::Duration(d));
                let err = expected_err(heap, "a non-negative duration", &dv);
                heap.drop_value(dv)?;
                return err;
            }
            let dv = heap.allocate(Data::Duration(d));
            heap.construct("Prelude.Result.Ok", vec![dv])
        }
        "decimal" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string that will be decoded as a BigDecimal", input);
            }
            match parse_decimal(&heap.string_lossy(input)?) {
                Some(d) => {
                    let v = heap.allocate(Data::BigDecimal {
                        value: d.value,
                        scale: d.scale,
                    });
                    heap.construct("Prelude.Result.Ok", vec![v])
                }
                None => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "bytesize" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string that will be decoded as a ByteSize", input);
            }
            match decode_bytesize(&heap.string_lossy(input)?) {
                Some(n) => {
                    let bi = heap.literal(&Lit::BigInt(n.to_str_radix(10)))?;
                    heap.construct("Prelude.Result.Ok", vec![bi])
                }
                None => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "bytesize_number" => {
            let Some(x) = number_of(input).filter(|v| v.is_finite()) else {
                return expected_err(heap, "a finite number", input);
            };
            #[expect(clippy::float_cmp, reason = "exact safe-integer check")]
            let ok = x >= 0.0 && x == x.trunc() && x <= MAX_SAFE;
            if !ok {
                return schema_error_single(heap, "", "an invalid value");
            }
            #[expect(clippy::cast_possible_truncation, reason = "bounded by 2^53 - 1")]
            let bi = heap.literal(&Lit::BigInt((x as i64).to_string()))?;
            heap.construct("Prelude.Result.Ok", vec![bi])
        }
        "bytesize_bigint" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string", input);
            }
            let text = heap.string_lossy(input)?;
            if !is_digits(&text, false) {
                return expected_err(heap, "a string matching the RegExp ^[0-9]+$", input);
            }
            let n = BigInt::parse_bytes(text.as_bytes(), 10).unwrap_or_else(BigInt::zero);
            let bi = heap.literal(&Lit::BigInt(n.to_str_radix(10)))?;
            heap.construct("Prelude.Result.Ok", vec![bi])
        }
        "tree_number" => match parse_tree_number(heap, input)? {
            Ok(f) => heap.construct("Prelude.Result.Ok", vec![Value::Float(f)]),
            Err(e) => Ok(e),
        },
        "tree_int" => match parse_tree_number(heap, input)? {
            Ok(f) => match check_i32(f) {
                Some(i) => heap.construct("Prelude.Result.Ok", vec![Value::Int(i)]),
                None => schema_error_single(
                    heap,
                    "",
                    "expected an Int, an integer from -2147483648 to 2147483647",
                ),
            },
            Err(e) => Ok(e),
        },
        "tree_bool" => {
            if is_str_val(heap, input)? {
                match heap.string_lossy(input)?.as_str() {
                    "true" => return heap.construct("Prelude.Result.Ok", vec![Value::Bool(true)]),
                    "false" => {
                        return heap.construct("Prelude.Result.Ok", vec![Value::Bool(false)]);
                    }
                    _ => {}
                }
            }
            expected_err(heap, "Union", input)
        }
        "tree_bigint" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string", input);
            }
            let text = heap.string_lossy(input)?;
            if !is_digits(&text, true) {
                return expected_err(heap, "a string representing a bigint", input);
            }
            let n = BigInt::parse_bytes(text.as_bytes(), 10).unwrap_or_else(BigInt::zero);
            let bi = heap.literal(&Lit::BigInt(n.to_str_radix(10)))?;
            heap.construct("Prelude.Result.Ok", vec![bi])
        }
        "datetime_utc"
        | "datetime_utc_millis"
        | "datetime_zoned"
        | "datetime_zone"
        | "datetime_zone_named"
        | "datetime_zone_offset" => super::datetime_ops::decode_datetime_leaf(heap, kind, input),
        _ => Err(Error::RangeError(format!("unknown leaf decoder: {kind}"))),
    }
}

fn bytesize_factor(unit: &str) -> Option<BigInt> {
    if matches!(unit, "B" | "byte" | "bytes") {
        return Some(BigInt::from(1));
    }
    let dec = [
        ("k", "kilo"),
        ("M", "mega"),
        ("G", "giga"),
        ("T", "tera"),
        ("P", "peta"),
        ("E", "exa"),
        ("Z", "zetta"),
        ("Y", "yotta"),
        ("R", "ronna"),
        ("Q", "quetta"),
    ];
    for (i, (sym, word)) in dec.iter().enumerate() {
        if unit == format!("{sym}B")
            || unit == format!("{word}byte")
            || unit == format!("{word}bytes")
        {
            return Some(BigInt::from(1000).pow(u32::try_from(i + 1).unwrap_or(1)));
        }
    }
    let bin = [
        ("K", "kibi"),
        ("M", "mebi"),
        ("G", "gibi"),
        ("T", "tebi"),
        ("P", "pebi"),
        ("E", "exbi"),
        ("Z", "zebi"),
        ("Y", "yobi"),
    ];
    for (i, (sym, word)) in bin.iter().enumerate() {
        if unit == format!("{sym}iB")
            || unit == format!("{word}byte")
            || unit == format!("{word}bytes")
        {
            return Some(BigInt::from(1024).pow(u32::try_from(i + 1).unwrap_or(1)));
        }
    }
    None
}

fn decode_bytesize(text: &str) -> Option<BigInt> {
    let t = text.trim();
    let unit_start = t.find(|c: char| c.is_ascii_alphabetic())?;
    let num_part = t[..unit_start].trim_end();
    let unit = &t[unit_start..];
    if !unit.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let factor = bytesize_factor(unit)?;
    let (whole, frac) = if let Some((w, f)) = num_part.split_once('.') {
        if w.is_empty()
            || f.is_empty()
            || !w.bytes().all(|b| b.is_ascii_digit())
            || !f.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        (w, f)
    } else {
        if num_part.is_empty() || !num_part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        (num_part, "")
    };
    let n = BigInt::parse_bytes(format!("{whole}{frac}").as_bytes(), 10)? * factor;
    let scale = pow10(i64::try_from(frac.len()).unwrap_or(0));
    (&n % &scale).is_zero().then(|| n / scale)
}
