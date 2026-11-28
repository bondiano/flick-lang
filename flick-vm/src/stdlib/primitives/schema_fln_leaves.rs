//! Fixed-width numeric leaf schemas and FLN scalar leaf decoders for `flick-vm`.

use num_bigint::BigInt;
use num_traits::ToPrimitive;

use super::json_ops::schema_error_single;
use super::schema_leaves::{is_digits, is_str_val, number_of};
use super::schema_ops::expected_err;
use super::{Data, Error, Heap, Value};

fn suffix_tag<'a>(heap: &'a Heap, opt: &Value) -> Result<Option<&'a str>, Error> {
    let Value::Object(tok) = opt else {
        return Ok(None);
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Ok(None);
    };
    if tag != "Prelude.Option.Some" || fields.len() != 1 {
        return Ok(None);
    }
    let Value::Object(inner) = &fields[0] else {
        return Ok(None);
    };
    let Data::Tagged { tag: suf, .. } = &heap.object(inner.id)?.data else {
        return Ok(None);
    };
    Ok(Some(suf.as_str()))
}

fn describe_fln(heap: &Heap, val: &Value) -> Result<String, Error> {
    let Value::Object(tok) = val else {
        return Ok("a non-FLN value".into());
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Ok("a non-FLN value".into());
    };
    Ok(match tag.as_str() {
        "Fln.Value.Unit" => "()".into(),
        "Fln.Value.Bool" => "Bool".into(),
        "Fln.Value.Int" => "Int".into(),
        "Fln.Value.BigInt" => "BigInt".into(),
        "Fln.Value.Float" => "Float".into(),
        "Fln.Value.String" => "String".into(),
        "Fln.Value.Tuple" => "Tuple".into(),
        "Fln.Value.List" => "List".into(),
        "Fln.Value.Record" => "Record".into(),
        "Fln.Value.Constructor" => {
            if fields.len() >= 2 {
                heap.string_lossy(&fields[1])
                    .unwrap_or_else(|_| "Constructor".into())
            } else {
                "Constructor".into()
            }
        }
        _ => "a non-FLN value".into(),
    })
}

fn fln_expected(heap: &mut Heap, what: &str, input: &Value) -> Result<Value, Error> {
    let desc = describe_fln(heap, input)?;
    schema_error_single(heap, "", &format!("expected {what}, got {desc}"))
}

fn extract_fln_bigint(heap: &Heap, val: &Value) -> Result<Option<BigInt>, Error> {
    let Value::Object(tok) = val else {
        return Ok(None);
    };
    let Data::BigInt(s) = &heap.object(tok.id)?.data else {
        return Ok(None);
    };
    Ok(BigInt::parse_bytes(s.as_bytes(), 10))
}

fn decode_fln_int(
    heap: &mut Heap,
    input: &Value,
    label: &str,
    want_suf: &str,
    min: i128,
    max: i128,
) -> Result<Result<i128, Value>, Error> {
    let Value::Object(tok) = input else {
        return Ok(Err(fln_expected(heap, label, input)?));
    };
    let (n_opt, suf_opt) = {
        let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
            return Ok(Err(fln_expected(heap, label, input)?));
        };
        if tag != "Fln.Value.Int" || fields.len() != 2 {
            return Ok(Err(fln_expected(heap, label, input)?));
        }
        let n = extract_fln_bigint(heap, &fields[0])?;
        let suf = suffix_tag(heap, &fields[1])?.map(str::to_owned);
        (n, suf)
    };
    let Some(bi) = n_opt else {
        return Ok(Err(fln_expected(heap, label, input)?));
    };
    if let Some(suf) = suf_opt
        && suf != want_suf
    {
        return Ok(Err(schema_error_single(
            heap,
            "",
            &format!("expected {label} suffix, got mismatched numeric suffix"),
        )?));
    }
    match bi.to_i128() {
        Some(v) if (min..=max).contains(&v) => Ok(Ok(v)),
        _ => Ok(Err(schema_error_single(
            heap,
            "",
            &format!("expected {label}, an integer from {min} to {max}"),
        )?)),
    }
}

#[expect(clippy::float_cmp, reason = "exact integer bounds check")]
fn check_bounded_f64(x: f64, min: f64, max: f64) -> bool {
    x.is_finite() && x == x.trunc() && x >= min && x <= max
}

#[expect(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded fixed-width numeric & FLN leaf decoders"
)]
pub(super) fn decode_extra_leaf(
    heap: &mut Heap,
    kind: &str,
    input: &Value,
) -> Option<Result<Value, Error>> {
    Some(match kind {
        "int8" => match number_of(input) {
            None => expected_err(heap, "an Int8", input),
            Some(x) if check_bounded_f64(x, -128.0, 127.0) => {
                heap.construct("Prelude.Result.Ok", vec![Value::Int8(x as i8)])
            }
            Some(_) => {
                schema_error_single(heap, "", "expected an Int8, an integer from -128 to 127")
            }
        },
        "int16" => match number_of(input) {
            None => expected_err(heap, "an Int16", input),
            Some(x) if check_bounded_f64(x, -32768.0, 32767.0) => {
                heap.construct("Prelude.Result.Ok", vec![Value::Int16(x as i16)])
            }
            Some(_) => schema_error_single(
                heap,
                "",
                "expected an Int16, an integer from -32768 to 32767",
            ),
        },
        "int64" => {
            if !is_str_val(heap, input).ok()? {
                return Some(expected_err(heap, "a string of digits", input));
            }
            let text = match heap.string_lossy(input) {
                Ok(t) => t,
                Err(e) => return Some(Err(e)),
            };
            if !is_digits(&text, true) {
                return Some(schema_error_single(heap, "", "expected a string of digits"));
            }
            match text.parse::<i64>() {
                Ok(n) => heap.construct("Prelude.Result.Ok", vec![Value::Int64(n)]),
                Err(_) => schema_error_single(
                    heap,
                    "",
                    "expected an Int64, an integer from -9223372036854775808 to 9223372036854775807",
                ),
            }
        }
        "uint8" => match number_of(input) {
            None => expected_err(heap, "a UInt8", input),
            Some(x) if check_bounded_f64(x, 0.0, 255.0) => {
                heap.construct("Prelude.Result.Ok", vec![Value::UInt8(x as u8)])
            }
            Some(_) => schema_error_single(heap, "", "expected a UInt8, an integer from 0 to 255"),
        },
        "uint16" => match number_of(input) {
            None => expected_err(heap, "a UInt16", input),
            Some(x) if check_bounded_f64(x, 0.0, 65535.0) => {
                heap.construct("Prelude.Result.Ok", vec![Value::UInt16(x as u16)])
            }
            Some(_) => {
                schema_error_single(heap, "", "expected a UInt16, an integer from 0 to 65535")
            }
        },
        "uint32" => match number_of(input) {
            None => expected_err(heap, "a UInt32", input),
            Some(x) if check_bounded_f64(x, 0.0, f64::from(u32::MAX)) => {
                heap.construct("Prelude.Result.Ok", vec![Value::UInt32(x as u32)])
            }
            Some(_) => schema_error_single(
                heap,
                "",
                "expected a UInt32, an integer from 0 to 4294967295",
            ),
        },
        "uint64" => {
            if !is_str_val(heap, input).ok()? {
                return Some(expected_err(heap, "a string of digits", input));
            }
            let text = match heap.string_lossy(input) {
                Ok(t) => t,
                Err(e) => return Some(Err(e)),
            };
            if !is_digits(&text, true) {
                return Some(schema_error_single(heap, "", "expected a string of digits"));
            }
            match text.parse::<u64>() {
                Ok(n) => heap.construct("Prelude.Result.Ok", vec![Value::UInt64(n)]),
                Err(_) => schema_error_single(
                    heap,
                    "",
                    "expected a UInt64, an integer from 0 to 18446744073709551615",
                ),
            }
        }
        "float32" => match number_of(input) {
            Some(x) if x.is_finite() && (x as f32).is_finite() => {
                heap.construct("Prelude.Result.Ok", vec![Value::Float32(x as f32)])
            }
            _ => expected_err(heap, "a finite Float32", input),
        },
        "fln_unit" => {
            let is_unit = matches!(
                input,
                Value::Object(tok)
                    if matches!(&heap.object(tok.id), Ok(obj) if matches!(&obj.data, Data::Tagged { tag, .. } if tag == "Fln.Value.Unit"))
            );
            if is_unit {
                heap.construct("Prelude.Result.Ok", vec![Value::Unit])
            } else {
                fln_expected(heap, "()", input)
            }
        }
        "fln_bool" => {
            let b = match input {
                Value::Object(tok) => match heap.object(tok.id) {
                    Ok(obj) => match &obj.data {
                        Data::Tagged { tag, fields, .. }
                            if tag == "Fln.Value.Bool" && fields.len() == 1 =>
                        {
                            match fields[0] {
                                Value::Bool(b) => Some(b),
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    Err(e) => return Some(Err(e)),
                },
                _ => None,
            };
            match b {
                Some(b) => heap.construct("Prelude.Result.Ok", vec![Value::Bool(b)]),
                None => fln_expected(heap, "a boolean", input),
            }
        }
        "fln_string" => {
            let s = match input {
                Value::Object(tok) => match heap.object(tok.id) {
                    Ok(obj) => match &obj.data {
                        Data::Tagged { tag, fields, .. }
                            if tag == "Fln.Value.String" && fields.len() == 1 =>
                        {
                            Some(fields[0].representation())
                        }
                        _ => None,
                    },
                    Err(e) => return Some(Err(e)),
                },
                _ => None,
            };
            match s {
                Some(sv) => match heap.dup(&sv) {
                    Ok(d) => heap.construct("Prelude.Result.Ok", vec![d]),
                    Err(e) => Err(e),
                },
                None => fln_expected(heap, "a string", input),
            }
        }
        "fln_bigint" => {
            let bi = match input {
                Value::Object(tok) => match heap.object(tok.id) {
                    Ok(obj) => match &obj.data {
                        Data::Tagged { tag, fields, .. }
                            if tag == "Fln.Value.BigInt" && fields.len() == 1 =>
                        {
                            Some(fields[0].representation())
                        }
                        _ => None,
                    },
                    Err(e) => return Some(Err(e)),
                },
                _ => None,
            };
            match bi {
                Some(bv) => match heap.dup(&bv) {
                    Ok(d) => heap.construct("Prelude.Result.Ok", vec![d]),
                    Err(e) => Err(e),
                },
                None => fln_expected(heap, "a BigInt", input),
            }
        }
        "fln_int" => match decode_fln_int(
            heap,
            input,
            "an Int",
            "Fln.NumSuffix.I32",
            i128::from(i32::MIN),
            i128::from(i32::MAX),
        ) {
            Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::Int(n as i32)]),
            Ok(Err(e)) => Ok(e),
            Err(e) => Err(e),
        },
        "fln_int8" => match decode_fln_int(heap, input, "an Int8", "Fln.NumSuffix.I8", -128, 127) {
            Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::Int8(n as i8)]),
            Ok(Err(e)) => Ok(e),
            Err(e) => Err(e),
        },
        "fln_int16" => {
            match decode_fln_int(heap, input, "an Int16", "Fln.NumSuffix.I16", -32768, 32767) {
                Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::Int16(n as i16)]),
                Ok(Err(e)) => Ok(e),
                Err(e) => Err(e),
            }
        }
        "fln_int64" => match decode_fln_int(
            heap,
            input,
            "an Int64",
            "Fln.NumSuffix.I64",
            i128::from(i64::MIN),
            i128::from(i64::MAX),
        ) {
            Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::Int64(n as i64)]),
            Ok(Err(e)) => Ok(e),
            Err(e) => Err(e),
        },
        "fln_uint8" => match decode_fln_int(heap, input, "a UInt8", "Fln.NumSuffix.U8", 0, 255) {
            Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::UInt8(n as u8)]),
            Ok(Err(e)) => Ok(e),
            Err(e) => Err(e),
        },
        "fln_uint16" => {
            match decode_fln_int(heap, input, "a UInt16", "Fln.NumSuffix.U16", 0, 65535) {
                Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::UInt16(n as u16)]),
                Ok(Err(e)) => Ok(e),
                Err(e) => Err(e),
            }
        }
        "fln_uint32" => match decode_fln_int(
            heap,
            input,
            "a UInt32",
            "Fln.NumSuffix.U32",
            0,
            i128::from(u32::MAX),
        ) {
            Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::UInt32(n as u32)]),
            Ok(Err(e)) => Ok(e),
            Err(e) => Err(e),
        },
        "fln_uint64" => match decode_fln_int(
            heap,
            input,
            "a UInt64",
            "Fln.NumSuffix.U64",
            0,
            i128::from(u64::MAX),
        ) {
            Ok(Ok(n)) => heap.construct("Prelude.Result.Ok", vec![Value::UInt64(n as u64)]),
            Ok(Err(e)) => Ok(e),
            Err(e) => Err(e),
        },
        "fln_number" => decode_fln_float(heap, input, false),
        "fln_float32" => decode_fln_float(heap, input, true),
        "fln_literal" => decode_fln_literal(heap, input),
        _ => return None,
    })
}

fn decode_fln_float(heap: &mut Heap, input: &Value, is_f32: bool) -> Result<Value, Error> {
    let label = if is_f32 {
        "a Float32"
    } else {
        "a finite number"
    };
    let Value::Object(tok) = input else {
        return fln_expected(heap, label, input);
    };
    let extracted = {
        let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
            return fln_expected(heap, label, input);
        };
        if tag == "Fln.Value.Float" && fields.len() == 2 {
            let suf = suffix_tag(heap, &fields[1])?;
            let ok_suf = if is_f32 {
                suf.is_none() || suf == Some("Fln.NumSuffix.F32")
            } else {
                suf.is_none()
            };
            if ok_suf && let Value::Float(x) = fields[0] {
                Some(x)
            } else {
                None
            }
        } else if tag == "Fln.Value.Int"
            && fields.len() == 2
            && suffix_tag(heap, &fields[1])?.is_none()
        {
            extract_fln_bigint(heap, &fields[0])?.and_then(|bi| bi.to_f64())
        } else {
            None
        }
    };
    match extracted {
        Some(x) if is_f32 => {
            #[expect(clippy::cast_possible_truncation, reason = "explicit f32 conversion")]
            let f = x as f32;
            if f.is_finite() {
                heap.construct("Prelude.Result.Ok", vec![Value::Float32(f)])
            } else {
                schema_error_single(heap, "", "expected a finite Float32")
            }
        }
        Some(x) if x.is_finite() => heap.construct("Prelude.Result.Ok", vec![Value::Float(x)]),
        Some(_) => schema_error_single(heap, "", "expected a finite number"),
        None => fln_expected(heap, label, input),
    }
}

fn decode_fln_literal(heap: &mut Heap, input: &Value) -> Result<Value, Error> {
    let Value::Object(tok) = input else {
        return fln_expected(heap, "a literal", input);
    };
    let extracted = {
        let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
            return fln_expected(heap, "a literal", input);
        };
        if tag == "Fln.Value.String" && fields.len() == 1 {
            Some(Ok(fields[0].representation()))
        } else if tag == "Fln.Value.Int" && fields.len() == 2 {
            extract_fln_bigint(heap, &fields[0])?.map(|bi| {
                if let Some(i) = bi.to_i32() {
                    Err(Value::Int(i))
                } else {
                    Err(Value::Float(bi.to_f64().unwrap_or(f64::NAN)))
                }
            })
        } else {
            None
        }
    };
    match extracted {
        Some(Ok(sv)) => {
            let dup = heap.dup(&sv)?;
            heap.construct("Prelude.Result.Ok", vec![dup])
        }
        Some(Err(num)) => heap.construct("Prelude.Result.Ok", vec![num]),
        None => fln_expected(heap, "a literal", input),
    }
}
