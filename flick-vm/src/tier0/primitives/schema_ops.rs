//! Counted `Schema`, `Codec`, `Check`, `Route`, and `Differ.json_patch` dispatch and core primitives.

use std::collections::BTreeSet;

use flick_core::ir::Lit;
use flick_core::ir::memory::SchemaIntrinsic;
use flick_core::opt::js_number;

use super::check_eval;
use super::check_ops;
use super::json_ops::{schema_error_single, stringify_value};
use super::json_schema_export;
use super::list_ops::slice_reps;
use super::schema_data;
use super::schema_leaves;
use super::schema_leaves_encode;
use super::schema_route_ops;
use super::{Data, Error, Heap, ObjectId, Value, charge};

#[expect(clippy::too_many_lines, reason = "central SchemaIntrinsic dispatcher")]
pub(super) fn run(
    heap: &mut Heap,
    op: SchemaIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        SchemaIntrinsic::JsonNull => Ok(heap.allocate(Data::JsonNull)),
        SchemaIntrinsic::CheckInert => check_inert(heap, &args[0], fuel),
        SchemaIntrinsic::DescribeInput => {
            let desc = describe_input(heap, &args[0])?;
            heap.literal(&Lit::String(desc))
        }
        SchemaIntrinsic::JoinErrorMessages => join_error_messages(heap, &args[0], fuel),
        SchemaIntrinsic::TagOf => schema_data::tag_of(heap, &args[0]),
        SchemaIntrinsic::DeconstructAdt => schema_data::deconstruct_adt(heap, &args[0], fuel),
        SchemaIntrinsic::RecordEntries => schema_data::record_entries(heap, &args[0], fuel),
        SchemaIntrinsic::RecordFromEntries => {
            schema_data::record_from_entries(heap, &args[0], false, fuel)
        }
        SchemaIntrinsic::RecordFromSortedEntries => {
            schema_data::record_from_entries(heap, &args[0], true, fuel)
        }
        SchemaIntrinsic::TupleToList => schema_data::tuple_to_list(heap, &args[0], fuel),
        SchemaIntrinsic::ListFromWire => schema_data::list_from_wire(heap, &args[0], fuel),
        SchemaIntrinsic::TupleFromList => schema_data::tuple_from_list(heap, &args[0], fuel),
        SchemaIntrinsic::IsJsonNull => Ok(Value::Bool(is_json_null(heap, &args[0])?)),
        SchemaIntrinsic::IsRedacted => schema_data::is_redacted(heap, &args[0]),
        SchemaIntrinsic::IsChunk => schema_data::is_chunk(heap, &args[0]),
        SchemaIntrinsic::CheckEmail => {
            let s = heap.string_lossy(&args[0])?;
            charge(fuel, s.len())?;
            Ok(Value::Bool(schema_data::is_email(&s)))
        }
        SchemaIntrinsic::CheckNonEmpty => {
            let units = heap.utf16_units(&args[0])?;
            Ok(Value::Bool(!units.is_empty()))
        }
        SchemaIntrinsic::CheckPositive => match args[0] {
            Value::Float(f) => Ok(Value::Bool(f > 0.0)),
            Value::Int(i) => Ok(Value::Bool(i > 0)),
            _ => Err(Error::ExpectedFloat),
        },
        SchemaIntrinsic::FormatValue => {
            let text = stringify_value(heap, &args[0], fuel)?;
            heap.literal(&Lit::String(text))
        }
        SchemaIntrinsic::RouteParseUrl => schema_route_ops::route_parse_url(heap, &args[0], fuel),
        SchemaIntrinsic::RouteCoerceCandidates => {
            schema_route_ops::route_coerce_candidates(heap, &args[0], fuel)
        }
        SchemaIntrinsic::RouteJsString => {
            let text = js_string_value(heap, &args[0], fuel)?;
            heap.literal(&Lit::String(text))
        }
        SchemaIntrinsic::Defect => {
            let msg = heap.string_lossy(&args[0])?;
            Err(Error::RangeError(msg))
        }
        SchemaIntrinsic::PrefixIssues => prefix_issues(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::CombineErrors => combine_errors(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::ConstructAdt => schema_data::construct_adt(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::RecordLookup => schema_data::record_lookup(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::DecodeLeaf => schema_leaves::decode_leaf(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::EncodeLeaf => {
            schema_leaves_encode::encode_leaf(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::ValidateLeaf => {
            schema_leaves_encode::validate_leaf(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::AcceptsLeaf => schema_leaves::accepts_leaf(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::LiteralAccepts => {
            schema_route_ops::literal_accepts(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::SplitDecode => {
            schema_route_ops::split_decode(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::SplitEncode => {
            schema_route_ops::split_encode(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::MakeCheck => check_ops::make_check(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::RunCheck => check_eval::run_check(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::SameValue => Ok(Value::Bool(same_value(heap, &args[0], &args[1])?)),
        SchemaIntrinsic::StrictSameValue => Ok(Value::Bool(super::strict_same_value(
            heap, &args[0], &args[1],
        )?)),
        SchemaIntrinsic::EncodeXml => schema_data::encode_xml(heap, &args[0], &args[1], fuel),
        SchemaIntrinsic::ExportJsonSchema => {
            json_schema_export::export_json_schema(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::RouteFormatUrl => {
            schema_route_ops::route_format_url(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::DifferJsonPatchOps => {
            schema_data::differ_patch_ops(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::DifferApplyJsonPatch => {
            schema_data::differ_apply_patch(heap, &args[0], &args[1], fuel)
        }
        SchemaIntrinsic::LiteralDecode => {
            schema_route_ops::literal_decode(heap, &args[0], &args[1], &args[2], fuel)
        }
        SchemaIntrinsic::SplitKvDecode => {
            schema_route_ops::split_kv_decode(heap, &args[0], &args[1], &args[2], fuel)
        }
        SchemaIntrinsic::SplitKvEncode => {
            schema_route_ops::split_kv_encode(heap, &args[0], &args[1], &args[2], fuel)
        }
    }
}

pub(super) fn is_json_null(heap: &Heap, val: &Value) -> Result<bool, Error> {
    Ok(matches!(
        val,
        Value::Object(tok) if matches!(heap.object(tok.id)?.data, Data::JsonNull)
    ))
}

pub(super) fn describe_input(heap: &Heap, val: &Value) -> Result<String, Error> {
    Ok(match val {
        Value::Unit => "undefined".into(),
        Value::Bool(_) => "boolean".into(),
        Value::Int(_) => "number".into(),
        Value::Float(f) => {
            if f.is_nan() || f.is_infinite() {
                js_number(*f)
            } else {
                "number".into()
            }
        }
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::JsonNull => "null".into(),
            Data::String(_) | Data::Utf16(_) => "string".into(),
            Data::BigInt(_) => "bigint".into(),
            Data::ListSlice { .. } => "an array".into(),
            Data::Tagged { tag, .. } if tag == "$list" || tag == "$tuple" => "an array".into(),
            Data::Closure { .. } => "function".into(),
            _ => "an object".into(),
        },
        _ => "an object".into(),
    })
}

pub(super) fn expected_err(heap: &mut Heap, what: &str, input: &Value) -> Result<Value, Error> {
    let desc = describe_input(heap, input)?;
    schema_error_single(heap, "", &format!("expected {what}, got {desc}"))
}

pub(super) fn js_string_value(heap: &Heap, val: &Value, fuel: &mut u64) -> Result<String, Error> {
    charge(fuel, 1)?;
    Ok(match val {
        Value::Unit => "undefined".into(),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => js_number(*f),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::JsonNull => "null".into(),
            Data::String(s) => s.clone(),
            Data::Utf16(u) => String::from_utf16_lossy(u),
            Data::BigInt(n) => n.clone(),
            Data::Duration(d) => super::duration_ops::format_duration(d),
            Data::BigDecimal { value, scale } => super::decimal_ops::format_decimal(value, *scale),
            _ => stringify_value(heap, val, fuel)?,
        },
        _ => String::new(),
    })
}

#[expect(clippy::float_cmp, reason = "JS === numeric equality semantics")]
pub(super) fn same_value(heap: &Heap, left: &Value, right: &Value) -> Result<bool, Error> {
    Ok(match (left, right) {
        (Value::Unit, Value::Unit) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Int(a), Value::Float(b)) => f64::from(*a) == *b,
        (Value::Float(a), Value::Int(b)) => *a == f64::from(*b),
        (Value::Object(a), Value::Object(b)) => {
            if a.id == b.id {
                return Ok(true);
            }
            match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                (Data::JsonNull, Data::JsonNull) => true,
                (Data::String(x), Data::String(y)) | (Data::BigInt(x), Data::BigInt(y)) => x == y,
                (Data::Utf16(x), Data::Utf16(y)) => x == y,
                (Data::String(x), Data::Utf16(y)) => x.encode_utf16().eq(y.iter().copied()),
                (Data::Utf16(x), Data::String(y)) => x.iter().copied().eq(y.encode_utf16()),
                _ => false,
            }
        }
        _ => false,
    })
}

fn check_inert(heap: &mut Heap, root: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let mut active = BTreeSet::<ObjectId>::new();
    let mut stack = vec![(root.representation(), String::new(), false)];
    while let Some((val, path, leaving)) = stack.pop() {
        charge(fuel, 1)?;
        if leaving {
            if let Some(id) = val.object_id() {
                active.remove(&id);
            }
            continue;
        }
        let Value::Object(ref tok) = val else {
            continue;
        };
        let id = tok.id;
        match &heap.object(id)?.data {
            Data::ForeignCycle { .. } => {
                let err = make_schema_error(heap, &path, "a cycle in the input")?;
                return heap.construct("Prelude.Option.Some", vec![err]);
            }
            Data::Record(fields) => {
                if !active.insert(id) {
                    let err = make_schema_error(heap, &path, "a cycle in the input")?;
                    return heap.construct("Prelude.Option.Some", vec![err]);
                }
                stack.push((val, String::new(), true));
                for (k, v) in fields.iter().rev() {
                    let next = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    stack.push((v.representation(), next, false));
                }
            }
            Data::Tagged { tag, fields, .. } if tag == "$list" && !fields.is_empty() => {
                if !active.insert(id) {
                    let err = make_schema_error(heap, &path, "a cycle in the input")?;
                    return heap.construct("Prelude.Option.Some", vec![err]);
                }
                stack.push((val, String::new(), true));
                for (i, elem) in fields.iter().enumerate().rev() {
                    stack.push((elem.representation(), format!("{path}[{i}]"), false));
                }
            }
            Data::ListSlice {
                backing,
                offset,
                len,
            } if *len > 0 => {
                let b = backing.object_id().ok_or(Error::PatternKind("list"))?;
                let (off, n) = (*offset, *len);
                if !active.insert(b) {
                    let err = make_schema_error(heap, &path, "a cycle in the input")?;
                    return heap.construct("Prelude.Option.Some", vec![err]);
                }
                let Data::Tagged { fields, .. } = &heap.object(b)?.data else {
                    return Err(Error::PatternKind("list"));
                };
                stack.push((backing.representation(), String::new(), true));
                for (i, elem) in fields[off..off + n].iter().enumerate().rev() {
                    stack.push((elem.representation(), format!("{path}[{i}]"), false));
                }
            }
            _ => {}
        }
    }
    heap.construct("Prelude.Option.None", vec![])
}

fn make_schema_error(heap: &mut Heap, path: &str, message: &str) -> Result<Value, Error> {
    let msg_val = heap.literal(&Lit::String(message.to_owned()))?;
    let path_str = if path.is_empty() { "(root)" } else { path };
    let path_val = match heap.literal(&Lit::String(path_str.to_owned())) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(msg_val)?;
            return Err(err);
        }
    };
    let issue = heap.record(vec![("message".into(), msg_val), ("path".into(), path_val)])?;
    let issues = heap.construct("$list", vec![issue])?;
    heap.construct("Prelude.SchemaError.SchemaError", vec![issues])
}

fn extract_issues(heap: &Heap, err: &Value) -> Result<Vec<(String, String)>, Error> {
    let Value::Object(tok) = err else {
        return Err(Error::PatternKind("SchemaError"));
    };
    let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("SchemaError"));
    };
    let list_rep = fields[0].representation();
    let reps = slice_reps(heap, &list_rep)?;
    let mut out = Vec::with_capacity(reps.len());
    for item in &reps {
        let Value::Object(itok) = item else {
            return Err(Error::PatternKind("SchemaIssue"));
        };
        let Data::Record(rfields) = &heap.object(itok.id)?.data else {
            return Err(Error::PatternKind("SchemaIssue"));
        };
        let msg = rfields
            .iter()
            .find(|(k, _)| k == "message")
            .map(|(_, v)| heap.string_lossy(v))
            .transpose()?
            .unwrap_or_default();
        let path = rfields
            .iter()
            .find(|(k, _)| k == "path")
            .map(|(_, v)| heap.string_lossy(v))
            .transpose()?
            .unwrap_or_default();
        out.push((path, msg));
    }
    Ok(out)
}

fn build_schema_error(heap: &mut Heap, issues: Vec<(String, String)>) -> Result<Value, Error> {
    let mut built = Vec::with_capacity(issues.len());
    for (path, msg) in issues {
        let msg_v = match heap.literal(&Lit::String(msg)) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_values(built)?;
                return Err(err);
            }
        };
        let p = if path.is_empty() {
            "(root)".to_owned()
        } else {
            path
        };
        let path_v = match heap.literal(&Lit::String(p)) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(msg_v)?;
                heap.drop_values(built)?;
                return Err(err);
            }
        };
        match heap.record(vec![("message".into(), msg_v), ("path".into(), path_v)]) {
            Ok(rec) => built.push(rec),
            Err(err) => {
                heap.drop_values(built)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", built)?;
    heap.construct("Prelude.SchemaError.SchemaError", vec![list])
}

pub(super) fn prefix_issues(
    heap: &mut Heap,
    seg: &Value,
    err: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let at = match seg {
        Value::Int(i) => format!("[{i}]"),
        _ => heap.string_lossy(seg)?,
    };
    let issues = extract_issues(heap, err)?;
    charge(fuel, issues.len())?;
    let prefixed = issues
        .into_iter()
        .map(|(path, msg)| {
            let next = if path.is_empty() || path == "(root)" {
                at.clone()
            } else if path.starts_with('[') {
                format!("{at}{path}")
            } else {
                format!("{at}.{path}")
            };
            (next, msg)
        })
        .collect();
    build_schema_error(heap, prefixed)
}

fn combine_errors(
    heap: &mut Heap,
    left: &Value,
    right: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let mut a = extract_issues(heap, left)?;
    let b = extract_issues(heap, right)?;
    charge(fuel, a.len() + b.len())?;
    a.extend(b);
    build_schema_error(heap, a)
}

fn join_error_messages(heap: &mut Heap, err: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let issues = extract_issues(heap, err)?;
    charge(fuel, issues.len())?;
    let joined = issues
        .into_iter()
        .map(|(_, m)| m)
        .collect::<Vec<_>>()
        .join("; ");
    make_schema_error(heap, "", &joined)
}
