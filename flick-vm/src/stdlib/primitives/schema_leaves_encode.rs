//! Counted leaf wire encoders, validators, base64/hex/bytesize/case helpers for `Schema`/`Codec`.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use flick_core::ir::Lit;
use flick_core::opt::js_number;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

use super::decimal_ops::format_decimal;
use super::duration_ops::{is_infinite, is_negative, to_nanos_big};
use super::json_ops::schema_error_single;
use super::list_ops::slice_reps;
use super::schema_leaves::{MAX_SAFE, check_i32, is_json_tree, is_str_val, number_of};
use super::schema_ops::{expected_err, js_string_value, prefix_issues};
use super::schema_route_ops::encode_uri_component;
use super::string_ops::js_trim;
use super::{Data, DurationData, Error, Fuel, Heap, Value, charge};

#[expect(clippy::too_many_lines, reason = "centralized leaf encoder dispatch")]
pub(super) fn encode_leaf(
    heap: &mut Heap,
    kind_val: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let kind = heap.string_lossy(kind_val)?;
    match kind.as_str() {
        "number" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            if x.is_nan() || x.is_infinite() {
                schema_error_single(heap, "", &format!("{} has no JSON form", js_number(x)))
            } else {
                heap.construct("Prelude.Result.Ok", vec![Value::Float(x)])
            }
        }
        "int" => {
            let x = number_of(val).ok_or(Error::ExpectedInt)?;
            heap.construct("Prelude.Result.Ok", vec![Value::Float(x)])
        }
        "string" | "bool" | "unknown" | "to_lower" | "to_upper" | "capitalize" | "uncapitalize" => {
            let d = heap.dup(val)?;
            heap.construct("Prelude.Result.Ok", vec![d])
        }
        "bigint" | "bytesize_bigint" => {
            let s = js_string_value(heap, val, fuel)?;
            let sv = heap.literal(&Lit::String(s))?;
            heap.construct("Prelude.Result.Ok", vec![sv])
        }
        "unit" => {
            let n = heap.allocate(Data::JsonNull);
            heap.construct("Prelude.Result.Ok", vec![n])
        }
        "never" => expected_err(heap, "nothing (Never)", val),
        "redacted" => schema_error_single(
            heap,
            "",
            "a Redacted value has no JSON form without an explicit policy",
        ),
        "masked_redacted" => {
            let s = heap.literal(&Lit::String("<redacted>".into()))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "json" => {
            if is_json_tree(heap, val, fuel)? {
                let d = heap.dup(val)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "JSON value", val)
            }
        }
        "lossless_number" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            let out = if x.is_nan() {
                heap.literal(&Lit::String("NaN".into()))?
            } else if x == f64::INFINITY {
                heap.literal(&Lit::String("Infinity".into()))?
            } else if x == f64::NEG_INFINITY {
                heap.literal(&Lit::String("-Infinity".into()))?
            } else if x == 0.0 && x.is_sign_negative() {
                heap.literal(&Lit::String("-0".into()))?
            } else {
                Value::Float(x)
            };
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "bool_from_bit" => match *val {
            Value::Bool(b) => heap.construct(
                "Prelude.Result.Ok",
                vec![Value::Float(if b { 1.0 } else { 0.0 })],
            ),
            _ => expected_err(heap, "a boolean", val),
        },
        "trim" => {
            let s = heap.string_lossy(val)?;
            if js_trim(&s) == s {
                let d = heap.dup(val)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "a string with no leading or trailing whitespace", val)
            }
        }
        "snake_to_camel" => {
            let out = heap.literal(&Lit::String(camel_to_snake(&heap.string_lossy(val)?)))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "number_from_string" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            let out = heap.literal(&Lit::String(js_number(x)))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "finite_from_string" => {
            let x = number_of(val).ok_or(Error::ExpectedFloat)?;
            if !x.is_finite() {
                return expected_err(heap, "a finite number", val);
            }
            let out = heap.literal(&Lit::String(js_number(x)))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "base64_str" => {
            let out = heap.literal(&Lit::String(
                STANDARD.encode(heap.string_lossy(val)?.as_bytes()),
            ))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "base64url_str" => {
            let out = heap.literal(&Lit::String(
                URL_SAFE_NO_PAD.encode(heap.string_lossy(val)?.as_bytes()),
            ))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "hex_str" => {
            let out = heap.literal(&Lit::String(encode_hex(heap.string_lossy(val)?.as_bytes())))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "bytes_base64" => encode_byte_list(heap, val, |b| STANDARD.encode(b), fuel),
        "bytes_base64url" => encode_byte_list(heap, val, |b| URL_SAFE_NO_PAD.encode(b), fuel),
        "bytes_hex" => encode_byte_list(heap, val, encode_hex, fuel),
        "uri_component" => {
            let out = heap.literal(&Lit::String(encode_uri_component(&heap.string_lossy(val)?)))?;
            heap.construct("Prelude.Result.Ok", vec![out])
        }
        "char" => {
            if heap.utf16_units(val)?.len() == 1 {
                let d = heap.dup(val)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "a value with a length of 1", val)
            }
        }
        "natural" => decode_checked_number(heap, val, 0.0, None, None),
        "port" => decode_checked_number(heap, val, 0.0, Some(65535.0), Some("a port number")),
        "duration_millis" => {
            let d = expect_dur(heap, val)?;
            let ms = match &d {
                DurationData::Infinite => f64::INFINITY,
                DurationData::NegativeInfinite => f64::NEG_INFINITY,
                DurationData::Millis(m) => *m,
                DurationData::Nanos(n) => n.to_f64().unwrap_or(f64::INFINITY) / 1_000_000.0,
            };
            if ms < 0.0 {
                return expected_err(
                    heap,
                    "a value greater than or equal to 0",
                    &Value::Float(ms),
                );
            }
            if !ms.is_finite() {
                return schema_error_single(
                    heap,
                    "",
                    &format!("{} has no JSON form", js_number(ms)),
                );
            }
            heap.construct("Prelude.Result.Ok", vec![Value::Float(ms)])
        }
        "duration_nanos" => {
            let d = expect_dur(heap, val)?;
            if is_infinite(&d) {
                return schema_error_single(heap, "", "an invalid value");
            }
            let s = heap.literal(&Lit::String(to_nanos_big(&d).to_str_radix(10)))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "duration_string" => {
            let d = expect_dur(heap, val)?;
            if is_negative(&d) {
                return expected_err(heap, "a non-negative duration", val);
            }
            let text = match &d {
                DurationData::Infinite => "Infinity".into(),
                DurationData::NegativeInfinite => "-Infinity".into(),
                DurationData::Nanos(n) => format!("{} nanos", n.to_str_radix(10)),
                DurationData::Millis(m) => format!("{} millis", js_number(*m)),
            };
            let s = heap.literal(&Lit::String(text))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "decimal" => {
            let Value::Object(tok) = val else {
                return Err(Error::PatternKind("BigDecimal"));
            };
            let Data::BigDecimal { value, scale } = &heap.object(tok.id)?.data else {
                return Err(Error::PatternKind("BigDecimal"));
            };
            let s = heap.literal(&Lit::String(format_decimal(value, *scale)))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "bytesize" => {
            let n = expect_bigint(heap, val)?;
            let text = if n == BigInt::from(1) {
                "1 byte".into()
            } else {
                format!("{} bytes", n.to_str_radix(10))
            };
            let s = heap.literal(&Lit::String(text))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "bytesize_number" => {
            let n = expect_bigint(heap, val)?;
            match n.to_i64() {
                #[expect(clippy::cast_precision_loss, reason = "bounded by 2^53 - 1")]
                Some(i) if (0..=9_007_199_254_740_991).contains(&i) => {
                    heap.construct("Prelude.Result.Ok", vec![Value::Float(i as f64)])
                }
                _ => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "tree_number" | "tree_int" | "tree_bool" | "tree_bigint" => {
            let s = heap.literal(&Lit::String(js_string_value(heap, val, fuel)?))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "datetime_utc"
        | "datetime_utc_millis"
        | "datetime_zoned"
        | "datetime_zone"
        | "datetime_zone_named"
        | "datetime_zone_offset" => {
            super::datetime_ops::encode_datetime_leaf(heap, kind.as_str(), val)
        }
        _ => super::schema_fln_encode::encode_extra_leaf(heap, &kind, val)
            .unwrap_or_else(|| Err(Error::RangeError(format!("unknown leaf encoder: {kind}")))),
    }
}

pub(super) fn validate_leaf(
    heap: &mut Heap,
    kind_val: &Value,
    input: &Value,
    _fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let kind = heap.string_lossy(kind_val)?;
    match kind.as_str() {
        "number" => match *input {
            Value::Float(f) => heap.construct("Prelude.Result.Ok", vec![Value::Float(f)]),
            Value::Int(i) => heap.construct("Prelude.Result.Ok", vec![Value::Float(f64::from(i))]),
            _ => expected_err(heap, "a number", input),
        },
        "int" => match number_of(input) {
            None => expected_err(heap, "a number", input),
            Some(x) => match check_i32(x) {
                Some(i) => heap.construct("Prelude.Result.Ok", vec![Value::Int(i)]),
                None => schema_error_single(
                    heap,
                    "",
                    "expected an Int, an integer from -2147483648 to 2147483647",
                ),
            },
        },
        "string" => {
            if is_str_val(heap, input)? {
                let d = heap.dup(input)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "a string", input)
            }
        }
        "bool" => match *input {
            Value::Bool(b) => heap.construct("Prelude.Result.Ok", vec![Value::Bool(b)]),
            _ => expected_err(heap, "a boolean", input),
        },
        "bigint" => {
            if matches!(input, Value::Object(t) if matches!(heap.object(t.id)?.data, Data::BigInt(_)))
            {
                let d = heap.dup(input)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "a BigInt", input)
            }
        }
        "unit" => {
            if matches!(input, Value::Unit) {
                heap.construct("Prelude.Result.Ok", vec![Value::Unit])
            } else {
                expected_err(heap, "()", input)
            }
        }
        "never" => expected_err(heap, "nothing (Never)", input),
        "unknown" => {
            let d = heap.dup(input)?;
            heap.construct("Prelude.Result.Ok", vec![d])
        }
        "duration" => {
            if !matches!(input, Value::Object(t) if matches!(heap.object(t.id)?.data, Data::Duration(_)))
            {
                return expected_err(heap, "Duration", input);
            }
            if is_negative(&expect_dur(heap, input)?) {
                return expected_err(heap, "a non-negative duration", input);
            }
            let dup = heap.dup(input)?;
            heap.construct("Prelude.Result.Ok", vec![dup])
        }
        "decimal" => {
            if matches!(input, Value::Object(t) if matches!(heap.object(t.id)?.data, Data::BigDecimal { .. }))
            {
                let dup = heap.dup(input)?;
                heap.construct("Prelude.Result.Ok", vec![dup])
            } else {
                expected_err(heap, "BigDecimal", input)
            }
        }
        _ => super::schema_fln_encode::validate_extra_leaf(heap, &kind, input)
            .unwrap_or_else(|| Err(Error::RangeError(format!("unknown leaf validator: {kind}")))),
    }
}

fn encode_byte_list(
    heap: &mut Heap,
    val: &Value,
    write: impl FnOnce(&[u8]) -> String,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, val)?;
    charge(fuel, reps.len())?;
    let mut bytes = Vec::with_capacity(reps.len());
    for (i, r) in reps.iter().enumerate() {
        let Some(x) = number_of(r) else {
            let err = expected_err(heap, "a number", r)?;
            return prefix_result_err(heap, i, err, fuel);
        };
        #[expect(clippy::float_cmp, reason = "exact integer check")]
        if !x.is_finite() || x != x.trunc() || x.abs() > MAX_SAFE {
            let err = expected_err(heap, "an integer", r)?;
            return prefix_result_err(heap, i, err, fuel);
        }
        if !(0.0..=255.0).contains(&x) {
            let err = expected_err(heap, "a value between 0 and 255", r)?;
            return prefix_result_err(heap, i, err, fuel);
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0..=255"
        )]
        bytes.push(x as u8);
    }
    let s = heap.literal(&Lit::String(write(&bytes)))?;
    heap.construct("Prelude.Result.Ok", vec![s])
}

fn prefix_result_err(
    heap: &mut Heap,
    idx: usize,
    err_res: Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let Value::Object(tok) = &err_res else {
        return Ok(err_res);
    };
    let inner = match &heap.object(tok.id)?.data {
        Data::Tagged { fields, .. } if fields.len() == 1 => fields[0].representation(),
        _ => return Ok(err_res),
    };
    let seg = Value::Int(i32::try_from(idx).unwrap_or(i32::MAX));
    let prefixed = prefix_issues(heap, &seg, &inner, fuel);
    heap.drop_value(err_res)?;
    heap.construct("Prelude.Result.Err", vec![prefixed?])
}

#[expect(clippy::float_cmp, reason = "exact safe-integer check")]
pub(super) fn decode_checked_number(
    heap: &mut Heap,
    input: &Value,
    min: f64,
    max: Option<f64>,
    label: Option<&str>,
) -> Result<Value, Error> {
    let Some(x) = number_of(input) else {
        return expected_err(heap, "a number", input);
    };
    if !x.is_finite() || x != x.trunc() || x.abs() > MAX_SAFE {
        return expected_err(heap, "an integer", input);
    }
    if let Some(hi) = max {
        if x < min || x > hi {
            let what = label.map_or_else(
                || format!("a value between {} and {}", js_number(min), js_number(hi)),
                ToOwned::to_owned,
            );
            return expected_err(heap, &what, input);
        }
    } else if x < min {
        return expected_err(
            heap,
            &format!("a value greater than or equal to {}", js_number(min)),
            input,
        );
    }
    heap.construct("Prelude.Result.Ok", vec![Value::Float(x)])
}

pub(super) fn decode_b64_std(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(|&c| c != '\n' && c != '\r').collect();
    STANDARD.decode(clean).ok()
}

pub(super) fn decode_b64_url(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(|&c| c != '\n' && c != '\r').collect();
    if clean.len() % 4 == 1 {
        return None;
    }
    URL_SAFE_NO_PAD.decode(clean.trim_end_matches('=')).ok()
}

pub(super) fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.as_bytes().chunks_exact(2) {
        out.push((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?);
    }
    Some(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0xf)]));
    }
    out
}

pub(super) fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(super) fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for ch in s.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            for uc in ch.to_uppercase() {
                out.push(uc);
            }
            upper = false;
        } else {
            out.push(ch);
        }
    }
    if upper {
        out.push('_');
    }
    out
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(ch);
        }
    }
    out
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

fn expect_bigint(heap: &Heap, val: &Value) -> Result<BigInt, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("BigInt"));
    };
    let Data::BigInt(s) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("BigInt"));
    };
    BigInt::parse_bytes(s.as_bytes(), 10).ok_or_else(|| Error::InvalidBigInt(s.clone()))
}
