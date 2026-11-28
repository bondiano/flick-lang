//! Fixed-width numeric and FLN scalar leaf encoders and validators for `flick-vm`.

use flick_core::ir::Lit;
use flick_core::opt::js_number;

use super::json_ops::schema_error_single;
use super::schema_leaves::number_of;
use super::schema_ops::expected_err;
use super::{Data, Error, Heap, Value};

fn make_fln_int(heap: &mut Heap, n: i128, suf: Option<&str>) -> Result<Value, Error> {
    let bi = heap.literal(&Lit::BigInt(n.to_string()))?;
    let opt = match suf {
        None => heap.construct("Prelude.Option.None", vec![])?,
        Some(tag) => {
            let s = heap.construct(tag, vec![])?;
            heap.construct("Prelude.Option.Some", vec![s])?
        }
    };
    let v = heap.construct("Fln.Value.Int", vec![bi, opt])?;
    heap.construct("Prelude.Result.Ok", vec![v])
}

fn make_fln_float(heap: &mut Heap, x: f64, suf: Option<&str>) -> Result<Value, Error> {
    if !x.is_finite() {
        return schema_error_single(heap, "", &format!("{} has no FLN form", js_number(x)));
    }
    let opt = match suf {
        None => heap.construct("Prelude.Option.None", vec![])?,
        Some(tag) => {
            let s = heap.construct(tag, vec![])?;
            heap.construct("Prelude.Option.Some", vec![s])?
        }
    };
    let v = heap.construct("Fln.Value.Float", vec![Value::Float(x), opt])?;
    heap.construct("Prelude.Result.Ok", vec![v])
}

fn encode_fln_literal(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    match *val {
        Value::Int(n) => make_fln_int(heap, i128::from(n), None),
        #[expect(clippy::cast_possible_truncation, reason = "exact integer literal")]
        Value::Float(x) if x.is_finite() && x.fract() == 0.0 => make_fln_int(heap, x as i128, None),
        Value::Object(ref tok) => {
            let is_str = matches!(&heap.object(tok.id)?.data, Data::String(_) | Data::Utf16(_));
            if is_str {
                let d = heap.dup(val)?;
                let v = heap.construct("Fln.Value.String", vec![d])?;
                heap.construct("Prelude.Result.Ok", vec![v])
            } else {
                expected_err(heap, "a literal", val)
            }
        }
        _ => expected_err(heap, "a literal", val),
    }
}

#[expect(clippy::too_many_lines, reason = "fixed-width and FLN leaf encoders")]
pub(super) fn encode_extra_leaf(
    heap: &mut Heap,
    kind: &str,
    val: &Value,
) -> Option<Result<Value, Error>> {
    Some(match kind {
        "int8" => match *val {
            Value::Int8(n) => heap.construct("Prelude.Result.Ok", vec![Value::Float(f64::from(n))]),
            _ => expected_err(heap, "an Int8", val),
        },
        "int16" => match *val {
            Value::Int16(n) => {
                heap.construct("Prelude.Result.Ok", vec![Value::Float(f64::from(n))])
            }
            _ => expected_err(heap, "an Int16", val),
        },
        "int64" => match *val {
            Value::Int64(n) => {
                let s = match heap.literal(&Lit::String(n.to_string())) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                heap.construct("Prelude.Result.Ok", vec![s])
            }
            _ => expected_err(heap, "an Int64", val),
        },
        "uint8" => match *val {
            Value::UInt8(n) => {
                heap.construct("Prelude.Result.Ok", vec![Value::Float(f64::from(n))])
            }
            _ => expected_err(heap, "a UInt8", val),
        },
        "uint16" => match *val {
            Value::UInt16(n) => {
                heap.construct("Prelude.Result.Ok", vec![Value::Float(f64::from(n))])
            }
            _ => expected_err(heap, "a UInt16", val),
        },
        "uint32" => match *val {
            Value::UInt32(n) => {
                heap.construct("Prelude.Result.Ok", vec![Value::Float(f64::from(n))])
            }
            _ => expected_err(heap, "a UInt32", val),
        },
        "uint64" => match *val {
            Value::UInt64(n) => {
                let s = match heap.literal(&Lit::String(n.to_string())) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                heap.construct("Prelude.Result.Ok", vec![s])
            }
            _ => expected_err(heap, "a UInt64", val),
        },
        "float32" => match *val {
            Value::Float32(f) => {
                let x = f64::from(f);
                if x.is_finite() {
                    heap.construct("Prelude.Result.Ok", vec![Value::Float(x)])
                } else {
                    schema_error_single(heap, "", &format!("{} has no JSON form", js_number(x)))
                }
            }
            _ => expected_err(heap, "a Float32", val),
        },
        "fln_unit" => match heap.construct("Fln.Value.Unit", vec![]) {
            Ok(v) => heap.construct("Prelude.Result.Ok", vec![v]),
            Err(e) => Err(e),
        },
        "fln_bool" => match *val {
            Value::Bool(b) => match heap.construct("Fln.Value.Bool", vec![Value::Bool(b)]) {
                Ok(v) => heap.construct("Prelude.Result.Ok", vec![v]),
                Err(e) => Err(e),
            },
            _ => expected_err(heap, "a boolean", val),
        },
        "fln_string" => match heap.dup(val) {
            Ok(d) => match heap.construct("Fln.Value.String", vec![d]) {
                Ok(v) => heap.construct("Prelude.Result.Ok", vec![v]),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        },
        "fln_bigint" => match heap.dup(val) {
            Ok(d) => match heap.construct("Fln.Value.BigInt", vec![d]) {
                Ok(v) => heap.construct("Prelude.Result.Ok", vec![v]),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        },
        "fln_int" => match *val {
            Value::Int(n) => make_fln_int(heap, i128::from(n), None),
            _ => expected_err(heap, "an Int", val),
        },
        "fln_int8" => match *val {
            Value::Int8(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.I8")),
            _ => expected_err(heap, "an Int8", val),
        },
        "fln_int16" => match *val {
            Value::Int16(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.I16")),
            _ => expected_err(heap, "an Int16", val),
        },
        "fln_int64" => match *val {
            Value::Int64(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.I64")),
            _ => expected_err(heap, "an Int64", val),
        },
        "fln_uint8" => match *val {
            Value::UInt8(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.U8")),
            _ => expected_err(heap, "a UInt8", val),
        },
        "fln_uint16" => match *val {
            Value::UInt16(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.U16")),
            _ => expected_err(heap, "a UInt16", val),
        },
        "fln_uint32" => match *val {
            Value::UInt32(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.U32")),
            _ => expected_err(heap, "a UInt32", val),
        },
        "fln_uint64" => match *val {
            Value::UInt64(n) => make_fln_int(heap, i128::from(n), Some("Fln.NumSuffix.U64")),
            _ => expected_err(heap, "a UInt64", val),
        },
        "fln_number" => match number_of(val) {
            Some(x) => make_fln_float(heap, x, None),
            None => expected_err(heap, "a number", val),
        },
        "fln_float32" => match *val {
            Value::Float32(f) => make_fln_float(heap, f64::from(f), Some("Fln.NumSuffix.F32")),
            _ => expected_err(heap, "a Float32", val),
        },
        "fln_literal" => encode_fln_literal(heap, val),
        _ => return None,
    })
}

pub(super) fn validate_extra_leaf(
    heap: &mut Heap,
    kind: &str,
    input: &Value,
) -> Option<Result<Value, Error>> {
    Some(match kind {
        "int8" => match *input {
            Value::Int8(n) => heap.construct("Prelude.Result.Ok", vec![Value::Int8(n)]),
            _ => expected_err(heap, "an Int8", input),
        },
        "int16" => match *input {
            Value::Int16(n) => heap.construct("Prelude.Result.Ok", vec![Value::Int16(n)]),
            _ => expected_err(heap, "an Int16", input),
        },
        "int64" => match *input {
            Value::Int64(n) => heap.construct("Prelude.Result.Ok", vec![Value::Int64(n)]),
            _ => expected_err(heap, "an Int64", input),
        },
        "uint8" => match *input {
            Value::UInt8(n) => heap.construct("Prelude.Result.Ok", vec![Value::UInt8(n)]),
            _ => expected_err(heap, "a UInt8", input),
        },
        "uint16" => match *input {
            Value::UInt16(n) => heap.construct("Prelude.Result.Ok", vec![Value::UInt16(n)]),
            _ => expected_err(heap, "a UInt16", input),
        },
        "uint32" => match *input {
            Value::UInt32(n) => heap.construct("Prelude.Result.Ok", vec![Value::UInt32(n)]),
            _ => expected_err(heap, "a UInt32", input),
        },
        "uint64" => match *input {
            Value::UInt64(n) => heap.construct("Prelude.Result.Ok", vec![Value::UInt64(n)]),
            _ => expected_err(heap, "a UInt64", input),
        },
        "float32" => match *input {
            Value::Float32(f) => heap.construct("Prelude.Result.Ok", vec![Value::Float32(f)]),
            _ => expected_err(heap, "a Float32", input),
        },
        _ => return None,
    })
}
