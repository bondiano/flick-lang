//! Counted leaf wire decoders and `accepts` predicates for `Schema`/`Codec`.

use flick_core::ir::Lit;
use num_bigint::BigInt;
use num_traits::Zero;

use super::json_ops::schema_error_single;
use super::json_pointer_ops::decode_uri_component;
use super::list_ops::slice_reps;
use super::schema_leaves_encode::{
    decode_b64_std, decode_b64_url, decode_checked_number, decode_hex, snake_to_camel,
};
use super::schema_ops::{describe_input, expected_err, is_json_null};
use super::schema_std_leaves::decode_std_leaf;
use super::string_ops::js_trim;
use super::{Data, Error, Fuel, Heap, Value, charge};

pub(super) const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

pub(super) fn number_of(val: &Value) -> Option<f64> {
    match *val {
        Value::Float(f) => Some(f),
        Value::Int(i) => Some(f64::from(i)),
        _ => None,
    }
}

pub(super) fn is_str_val(heap: &Heap, val: &Value) -> Result<bool, Error> {
    Ok(matches!(
        val,
        Value::Object(tok)
            if matches!(heap.object(tok.id)?.data, Data::String(_) | Data::Utf16(_))
    ))
}

pub(super) fn is_json_tree(heap: &Heap, val: &Value, fuel: Fuel<'_>) -> Result<bool, Error> {
    charge(fuel, 1)?;
    match val {
        Value::Bool(_) | Value::Int(_) => Ok(true),
        Value::Float(f) => Ok(f.is_finite()),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::JsonNull | Data::String(_) | Data::Utf16(_) => Ok(true),
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                for f in fields {
                    if !is_json_tree(heap, f, fuel)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Data::ListSlice { .. } => {
                for elem in &slice_reps(heap, val)? {
                    if !is_json_tree(heap, elem, fuel)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Data::Record(fields) => {
                for (_, v) in fields {
                    if !is_json_tree(heap, v, fuel)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        },
        _ => Ok(false),
    }
}

pub(super) fn is_digits(s: &str, signed: bool) -> bool {
    let rest = if signed {
        s.strip_prefix('-').unwrap_or(s)
    } else {
        s
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

#[expect(clippy::float_cmp, reason = "exact integer bounds check")]
pub(super) fn check_i32(x: f64) -> Option<i32> {
    if x.is_finite()
        && x == x.trunc()
        && x >= f64::from(i32::MIN)
        && x <= f64::from(i32::MAX)
        && !(x == 0.0 && x.is_sign_negative())
    {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "bounded by i32::MIN..=i32::MAX"
        )]
        Some(x as i32)
    } else {
        None
    }
}

pub(super) fn js_number_parse(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    if t == "Infinity" || t == "+Infinity" {
        return f64::INFINITY;
    }
    if t == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "JS Number hex literal semantics"
        )]
        return u64::from_str_radix(hex, 16).map_or(f64::NAN, |n| n as f64);
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

pub(super) fn is_finite_text(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
        i += 1;
    }
    let mut has_digits = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        has_digits = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let mut frac = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            frac = true;
            i += 1;
        }
        has_digits = frac;
    }
    if !has_digits {
        return false;
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        i += 1;
        if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
            i += 1;
        }
        let mut exp = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            exp = true;
            i += 1;
        }
        if !exp {
            return false;
        }
    }
    i == bytes.len()
}

pub(super) fn parse_tree_number(
    heap: &mut Heap,
    input: &Value,
) -> Result<Result<f64, Value>, Error> {
    if !is_str_val(heap, input)? {
        return Ok(Err(expected_err(heap, "Union", input)?));
    }
    let s = heap.string_lossy(input)?;
    if is_finite_text(&s) {
        return Ok(Ok(s.parse::<f64>().unwrap_or(f64::NAN)));
    }
    match s.as_str() {
        "NaN" => Ok(Ok(f64::NAN)),
        "Infinity" => Ok(Ok(f64::INFINITY)),
        "-Infinity" => Ok(Ok(f64::NEG_INFINITY)),
        _ => {
            let desc = describe_input(heap, input)?;
            let i1 = format!("expected a string representing a finite number, got {desc}");
            let i2 = format!("expected Union, got {desc}");
            let m1 = heap.literal(&Lit::String(i1))?;
            let p1 = heap.literal(&Lit::String("(root)".into()))?;
            let r1 = heap.record(vec![("message".into(), m1), ("path".into(), p1)])?;
            let m2 = heap.literal(&Lit::String(i2))?;
            let p2 = heap.literal(&Lit::String("(root)".into()))?;
            let r2 = heap.record(vec![("message".into(), m2), ("path".into(), p2)])?;
            let list = heap.construct("$list", vec![r1, r2])?;
            let err = heap.construct("Prelude.SchemaError.SchemaError", vec![list])?;
            Ok(Err(heap.construct("Prelude.Result.Err", vec![err])?))
        }
    }
}

pub(super) fn accepts_leaf(
    heap: &Heap,
    kind_val: &Value,
    input: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let kind = heap.string_lossy(kind_val)?;
    let ok = match kind.as_str() {
        "number"
        | "int"
        | "natural"
        | "port"
        | "duration_millis"
        | "bytesize_number"
        | "datetime_utc_millis"
        | "datetime_zone_offset" => number_of(input).is_some(),
        "string"
        | "bigint"
        | "trim"
        | "to_lower"
        | "to_upper"
        | "capitalize"
        | "uncapitalize"
        | "snake_to_camel"
        | "number_from_string"
        | "finite_from_string"
        | "base64_str"
        | "base64url_str"
        | "hex_str"
        | "bytes_base64"
        | "bytes_base64url"
        | "bytes_hex"
        | "uri_component"
        | "char"
        | "duration_nanos"
        | "duration_string"
        | "decimal"
        | "bytesize"
        | "bytesize_bigint"
        | "datetime_utc"
        | "datetime_zoned"
        | "datetime_zone"
        | "datetime_zone_named" => is_str_val(heap, input)?,
        "bool" => matches!(input, Value::Bool(_)),
        "unit" => is_json_null(heap, input)?,
        "never" => false,
        "json" => is_json_tree(heap, input, fuel)?,
        "lossless_number" => {
            number_of(input).is_some()
                || (is_str_val(heap, input)?
                    && matches!(
                        heap.string_lossy(input)?.as_str(),
                        "NaN" | "Infinity" | "-Infinity" | "-0"
                    ))
        }
        "bool_from_bit" => matches!(number_of(input), Some(0.0 | 1.0)),
        "masked_redacted" => is_str_val(heap, input)? && heap.string_lossy(input)? == "<redacted>",
        _ => true,
    };
    Ok(Value::Bool(ok))
}

#[expect(clippy::too_many_lines, reason = "centralized leaf decoder dispatch")]
pub(super) fn decode_leaf(
    heap: &mut Heap,
    kind_val: &Value,
    input: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let kind = heap.string_lossy(kind_val)?;
    match kind.as_str() {
        "number" => match number_of(input) {
            Some(x) if x.is_finite() => heap.construct("Prelude.Result.Ok", vec![Value::Float(x)]),
            _ => expected_err(heap, "a finite number", input),
        },
        "int" => match number_of(input) {
            None => expected_err(heap, "an Int", input),
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
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string of digits", input);
            }
            let text = heap.string_lossy(input)?;
            if !is_digits(&text, true) {
                return schema_error_single(heap, "", "expected a string of digits");
            }
            let n = BigInt::parse_bytes(text.as_bytes(), 10).unwrap_or_else(BigInt::zero);
            let bi = heap.literal(&Lit::BigInt(n.to_str_radix(10)))?;
            heap.construct("Prelude.Result.Ok", vec![bi])
        }
        "unit" => {
            if is_json_null(heap, input)? {
                heap.construct("Prelude.Result.Ok", vec![Value::Unit])
            } else {
                expected_err(heap, "null", input)
            }
        }
        "never" => expected_err(heap, "nothing (Never)", input),
        "unknown" => {
            let d = heap.dup(input)?;
            heap.construct("Prelude.Result.Ok", vec![d])
        }
        "redacted" => schema_error_single(
            heap,
            "",
            "a Redacted value has no JSON form without an explicit policy",
        ),
        "masked_redacted" => {
            if !is_str_val(heap, input)? || heap.string_lossy(input)? != "<redacted>" {
                expected_err(heap, "\"<redacted>\"", input)
            } else {
                schema_error_single(heap, "", "a masked Redacted value cannot be decoded")
            }
        }
        "json" => {
            if is_json_tree(heap, input, fuel)? {
                let d = heap.dup(input)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "JSON value", input)
            }
        }
        "lossless_number" => {
            if let Some(x) = number_of(input) {
                return heap.construct("Prelude.Result.Ok", vec![Value::Float(x)]);
            }
            if is_str_val(heap, input)? {
                let s = heap.string_lossy(input)?;
                let mapped = match s.as_str() {
                    "NaN" => Some(f64::NAN),
                    "Infinity" => Some(f64::INFINITY),
                    "-Infinity" => Some(f64::NEG_INFINITY),
                    "-0" => Some(-0.0),
                    _ => None,
                };
                if let Some(f) = mapped {
                    return heap.construct("Prelude.Result.Ok", vec![Value::Float(f)]);
                }
            }
            expected_err(heap, "a number", input)
        }
        "bool_from_bit" => match number_of(input) {
            Some(0.0) => heap.construct("Prelude.Result.Ok", vec![Value::Bool(false)]),
            Some(1.0) => heap.construct("Prelude.Result.Ok", vec![Value::Bool(true)]),
            _ => expected_err(heap, "Union", input),
        },
        "trim" => decode_str_as(
            heap,
            input,
            "a string that will be decoded as a trimmed string",
            |s| Ok(js_trim(s).to_owned()),
        ),
        "to_lower" => decode_str_as(heap, input, "a string", |s| Ok(s.to_lowercase())),
        "to_upper" => decode_str_as(heap, input, "a string", |s| Ok(s.to_uppercase())),
        "capitalize" => decode_str_as(heap, input, "a string", |s| {
            let mut chars = s.chars();
            Ok(match chars.next() {
                None => String::new(),
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
            })
        }),
        "uncapitalize" => decode_str_as(heap, input, "a string", |s| {
            let mut chars = s.chars();
            Ok(match chars.next() {
                None => String::new(),
                Some(c) => format!("{}{}", c.to_lowercase(), chars.as_str()),
            })
        }),
        "snake_to_camel" => decode_str_as(heap, input, "a string", |s| Ok(snake_to_camel(s))),
        "number_from_string" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string that will be decoded as a number", input);
            }
            let s = heap.string_lossy(input)?;
            heap.construct("Prelude.Result.Ok", vec![Value::Float(js_number_parse(&s))])
        }
        "finite_from_string" => {
            if !is_str_val(heap, input)? {
                return expected_err(
                    heap,
                    "a string that will be decoded as a finite number",
                    input,
                );
            }
            let f = js_number_parse(&heap.string_lossy(input)?);
            if f.is_finite() {
                heap.construct("Prelude.Result.Ok", vec![Value::Float(f)])
            } else {
                expected_err(heap, "a finite number", &Value::Float(f))
            }
        }
        "base64_str" => decode_str_bytes(
            heap,
            input,
            "a base64 encoded string that will be decoded as a UTF-8 string",
            decode_b64_std,
        ),
        "base64url_str" => decode_str_bytes(
            heap,
            input,
            "a base64 (URL) encoded string that will be decoded as a UTF-8 string",
            decode_b64_url,
        ),
        "hex_str" => decode_str_bytes(
            heap,
            input,
            "a hex encoded string that will be decoded as a UTF-8 string",
            decode_hex,
        ),
        "bytes_base64" => decode_byte_list(
            heap,
            input,
            "a base64 encoded string that will be decoded as Uint8Array",
            decode_b64_std,
            fuel,
        ),
        "bytes_base64url" => decode_byte_list(
            heap,
            input,
            "a base64 (URL) encoded string that will be decoded as Uint8Array",
            decode_b64_url,
            fuel,
        ),
        "bytes_hex" => decode_byte_list(
            heap,
            input,
            "a hex encoded string that will be decoded as Uint8Array",
            decode_hex,
            fuel,
        ),
        "uri_component" => decode_str_as(
            heap,
            input,
            "a URI component encoded string that will be decoded as a UTF-8 string",
            |s| decode_uri_component(s).ok_or("an invalid value"),
        ),
        "char" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "a string", input);
            }
            if heap.utf16_units(input)?.len() == 1 {
                let d = heap.dup(input)?;
                heap.construct("Prelude.Result.Ok", vec![d])
            } else {
                expected_err(heap, "a value with a length of 1", input)
            }
        }
        "natural" => decode_checked_number(heap, input, 0.0, None, None),
        "port" => decode_checked_number(heap, input, 0.0, Some(65535.0), Some("a port number")),
        _ => decode_std_leaf(heap, &kind, input),
    }
}

fn decode_str_as(
    heap: &mut Heap,
    input: &Value,
    what: &str,
    f: impl FnOnce(&str) -> Result<String, &'static str>,
) -> Result<Value, Error> {
    if !is_str_val(heap, input)? {
        return expected_err(heap, what, input);
    }
    match f(&heap.string_lossy(input)?) {
        Ok(out) => {
            let v = heap.literal(&Lit::String(out))?;
            heap.construct("Prelude.Result.Ok", vec![v])
        }
        Err(msg) => schema_error_single(heap, "", msg),
    }
}

fn decode_str_bytes(
    heap: &mut Heap,
    input: &Value,
    what: &str,
    read: impl FnOnce(&str) -> Option<Vec<u8>>,
) -> Result<Value, Error> {
    decode_str_as(heap, input, what, |s| {
        let bytes = read(s).ok_or("an invalid value")?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
}

fn decode_byte_list(
    heap: &mut Heap,
    input: &Value,
    what: &str,
    read: impl FnOnce(&str) -> Option<Vec<u8>>,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    if !is_str_val(heap, input)? {
        return expected_err(heap, what, input);
    }
    let Some(bytes) = read(&heap.string_lossy(input)?) else {
        return schema_error_single(heap, "", "an invalid value");
    };
    charge(fuel, bytes.len())?;
    let elems: Vec<Value> = bytes
        .into_iter()
        .map(|b| Value::Int(i32::from(b)))
        .collect();
    let list = heap.construct("$list", elems)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}
