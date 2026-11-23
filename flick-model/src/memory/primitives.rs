//! Value operations borrow their inputs; the machine owns and releases them.
use std::cmp::Ordering;
use std::collections::BTreeSet;

use flick_core::ir::memory::Intrinsic;
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{
    Arith, Num, int_arith, int8_arith, int16_arith, int64_arith, uint8_arith, uint16_arith,
    uint32_arith, uint64_arith,
};
use flick_core::opt::{js_number, json_string};

use super::{
    CronData, Data, DateTimeData, DurationData, Error, ExitData, GraphData, GraphEdge, GraphNode,
    GraphWalkerData, Heap, ObjectId, ObjectToken, TimeZoneData, Value, charge, tagged_boundary,
};

#[path = "numeric.rs"]
mod numeric;

#[path = "scalar.rs"]
mod scalar;

#[path = "bigint.rs"]
mod bigint;

#[path = "bigint_conversion.rs"]
mod bigint_conversion;

#[path = "host.rs"]
mod host;

#[path = "primitives/int_ops.rs"]
mod int_ops;

#[path = "primitives/fixed_width_ops.rs"]
mod fixed_width_ops;

#[path = "primitives/float_ops.rs"]
mod float_ops;

#[path = "primitives/bigint_ops.rs"]
mod bigint_ops;

#[path = "primitives/string_case.rs"]
mod string_case;

#[path = "primitives/string_ops.rs"]
mod string_ops;

#[path = "primitives/list_ops.rs"]
mod list_ops;

#[path = "primitives/list_ops_extra.rs"]
mod list_ops_extra;

#[path = "primitives/record_ops.rs"]
mod record_ops;

#[path = "primitives/debug_ops.rs"]
mod debug_ops;

#[path = "primitives/bytes_ops.rs"]
mod bytes_ops;

#[path = "primitives/json_ops.rs"]
mod json_ops;

#[path = "primitives/json_pointer_ops.rs"]
mod json_pointer_ops;

#[path = "primitives/json_patch_ops.rs"]
mod json_patch_ops;

#[path = "primitives/decimal_ops.rs"]
mod decimal_ops;

#[path = "primitives/decimal_round.rs"]
mod decimal_round;

#[path = "primitives/regexp_ops.rs"]
mod regexp_ops;

#[path = "primitives/duration_ops.rs"]
mod duration_ops;

#[path = "primitives/outcome_ops.rs"]
mod outcome_ops;

#[path = "primitives/cause_ops.rs"]
mod cause_ops;

#[path = "primitives/cause_pretty.rs"]
mod cause_pretty;

#[path = "primitives/crypto_ops.rs"]
mod crypto_ops;

#[path = "primitives/config_url.rs"]
mod config_url;

#[path = "primitives/config_ops.rs"]
mod config_ops;

#[path = "primitives/arbitrary_ops.rs"]
mod arbitrary_ops;

#[path = "primitives/arbitrary_shrink.rs"]
mod arbitrary_shrink;

#[path = "primitives/arbitrary_fmt.rs"]
mod arbitrary_fmt;

#[path = "primitives/gen_ops.rs"]
mod gen_ops;

#[path = "primitives/schema_ops.rs"]
mod schema_ops;

#[path = "primitives/schema_data.rs"]
mod schema_data;

#[path = "primitives/schema_leaves.rs"]
mod schema_leaves;

#[path = "primitives/schema_std_leaves.rs"]
mod schema_std_leaves;

#[path = "primitives/schema_leaves_encode.rs"]
mod schema_leaves_encode;

#[path = "primitives/schema_route_ops.rs"]
mod schema_route_ops;

#[path = "primitives/check_ops.rs"]
mod check_ops;

#[path = "primitives/check_eval.rs"]
mod check_eval;

#[path = "primitives/json_schema_jval.rs"]
mod json_schema_jval;

#[path = "primitives/json_schema_export.rs"]
mod json_schema_export;

#[path = "primitives/json_schema_dialect.rs"]
mod json_schema_dialect;

#[path = "primitives/json_schema_conv.rs"]
mod json_schema_conv;

#[path = "primitives/bigint_norm.rs"]
mod bigint_norm;

#[path = "primitives/json_schema_to.rs"]
mod json_schema_to;

#[path = "primitives/path_ops.rs"]
mod path_ops;

#[path = "primitives/path_url.rs"]
mod path_url;

#[path = "primitives/datetime_date.rs"]
mod datetime_date;

#[path = "primitives/datetime_tzif.rs"]
mod datetime_tzif;

#[path = "primitives/datetime_tzif_bin.rs"]
mod datetime_tzif_bin;

#[path = "primitives/datetime_parse.rs"]
mod datetime_parse;

#[path = "primitives/datetime_parse_scan.rs"]
mod datetime_parse_scan;

#[path = "primitives/datetime_format.rs"]
mod datetime_format;

#[path = "primitives/datetime_ops.rs"]
mod datetime_ops;

#[path = "primitives/datetime_parts.rs"]
mod datetime_parts;

#[path = "primitives/cron_ops.rs"]
mod cron_ops;

#[path = "primitives/cron_step.rs"]
mod cron_step;

#[path = "primitives/graph_ops.rs"]
mod graph_ops;

#[path = "primitives/graph_json.rs"]
mod graph_json;

#[path = "primitives/graph_build.rs"]
mod graph_build;

#[path = "primitives/graph_transform.rs"]
mod graph_transform;

#[path = "primitives/graph_combine.rs"]
mod graph_combine;

#[path = "primitives/graph_query.rs"]
mod graph_query;

#[path = "primitives/graph_walk.rs"]
mod graph_walk;

#[path = "primitives/graph_algos.rs"]
mod graph_algos;

#[path = "primitives/graph_conn.rs"]
mod graph_conn;

#[path = "primitives/graph_paths.rs"]
mod graph_paths;

#[path = "primitives/graph_allpaths.rs"]
mod graph_allpaths;

#[path = "primitives/graph_flow.rs"]
mod graph_flow;

#[path = "primitives/isaac_ops.rs"]
mod isaac_ops;

pub(super) use bigint_norm::bigint_pattern_eq;
use bigint_norm::{bigint_order, bigint_text, normalize_bigint};
pub(super) use cause_ops::{
    concat_causes, first_defect_message, join_causes, strip_fail_and_add_interrupt,
};
pub(super) use decimal_ops::format_decimal;
pub use host::HostMode;
pub(super) use isaac_ops::{
    IsaacRng, describe_cause_report, duration_sleep_nanos, fails_to_dies_cause,
    without_interrupts_cause,
};
pub(super) use outcome_ops::describe_defect;

pub(super) fn metered_primitive(
    heap: &mut Heap,
    kind: Intrinsic,
    arguments: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    heap.admit_values(arguments)?;
    match kind {
        Intrinsic::ToBigInt => bigint_conversion::run(heap, &arguments[0], fuel),
        Intrinsic::ArrayToList => array_to_list(heap, &arguments[0], fuel),
        Intrinsic::PrimitiveCompare => numeric::compare(heap, &arguments[0], &arguments[1], fuel),
        Intrinsic::PrimitiveHash => Ok(Value::Int(numeric::hash(heap, &arguments[0], fuel)?)),
        Intrinsic::NumberToFloat | Intrinsic::FloatMin | Intrinsic::FloatMax => {
            scalar::run(heap, kind, arguments, fuel)
        }
        Intrinsic::IntUnsafeDiv | Intrinsic::IntUnsafeRem => {
            unsafe_int(kind, &arguments[0], &arguments[1])
        }
        Intrinsic::StringLength => {
            let units = heap.utf16_units(&arguments[0])?;
            super::charge(fuel, units.len())?;
            Ok(Value::Int(i32::try_from(units.len()).unwrap_or(i32::MAX)))
        }
        Intrinsic::ListRange => list_range(heap, &arguments[0], &arguments[1], fuel),
        Intrinsic::RecordGet => record_get(heap, &arguments[0], &arguments[1], fuel),
        Intrinsic::RecordExtend => {
            record_extend(heap, &arguments[0], &arguments[1], &arguments[2], fuel)
        }
        Intrinsic::RecordRemove => record_remove(heap, &arguments[0], &arguments[1], fuel),
        Intrinsic::RecordProject => record_project(heap, &arguments[0], &arguments[1], fuel),
        Intrinsic::RecordMergeDisjoint => {
            record_merge_disjoint(heap, &arguments[0], &arguments[1], fuel)
        }
        Intrinsic::RecordLabels => record_labels(heap, &arguments[0], fuel),
        Intrinsic::ExitIsSuccess | Intrinsic::ExitIsFailure | Intrinsic::ExitHasInterrupts => {
            exit_predicate(heap, kind, &arguments[0], fuel)
        }
        Intrinsic::Int(op) => int_ops::run(heap, op, arguments, fuel),
        Intrinsic::FixedWidth(op) => fixed_width_ops::run(heap, op, arguments, fuel),
        Intrinsic::Float(op) => float_ops::run(heap, op, arguments, fuel),
        Intrinsic::BigInt(op) => bigint_ops::run(heap, op, arguments, fuel),
        Intrinsic::String(op) => string_ops::run(heap, op, arguments, fuel),
        Intrinsic::List(op) => list_ops::run(heap, op, arguments, fuel),
        Intrinsic::Record(op) => record_ops::run(heap, op, arguments, fuel),
        Intrinsic::Debug(op) => debug_ops::run(heap, op, arguments, fuel),
        Intrinsic::Bytes(op) => bytes_ops::run(heap, op, arguments, fuel),
        Intrinsic::Hash(op) => numeric::hash_op(heap, op, arguments, fuel),
        Intrinsic::Json(op) => json_ops::run(heap, op, arguments, fuel),
        Intrinsic::BigDecimal(op) => decimal_ops::run(heap, op, arguments, fuel),
        Intrinsic::RegExp(op) => regexp_ops::run(heap, op, arguments, fuel),
        Intrinsic::Duration(op) => duration_ops::run(heap, op, arguments, fuel),
        Intrinsic::Outcome(op) => outcome_ops::run(heap, op, arguments, fuel),
        Intrinsic::Gen(op) => gen_ops::run(heap, op, arguments, fuel),
        Intrinsic::Schema(op) => schema_ops::run(heap, op, arguments, fuel),
        Intrinsic::JsonSchema(op) => json_schema_dialect::run(heap, op, arguments, fuel),
        Intrinsic::Path(op) => path_ops::run(heap, op, arguments, fuel),
        Intrinsic::DateTime(op) => datetime_ops::run(heap, op, arguments, fuel),
        Intrinsic::Cron(op) => cron_ops::run(heap, op, arguments, fuel),
        Intrinsic::Graph(op) => graph_ops::run(heap, op, arguments, fuel),
        Intrinsic::Crypto(op) => crypto_ops::run(heap, op, arguments, fuel),
        Intrinsic::Config(op) => config_ops::run(heap, op, arguments, fuel),
        Intrinsic::Arbitrary(op) => arbitrary_ops::run(heap, op, arguments, fuel),
        _ => Err(unsupported("not a numeric primitive")),
    }
}

include!("primitives/records.rs");

/// Both native backends represent readonly tuples and lists as arrays. Keep
/// ordinary pattern kinds strict; bridge only this audited observer boundary.
pub(super) fn array_to_list(
    heap: &mut Heap,
    value: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    let Value::Object(token) = value else {
        return Err(unsupported("array observer requires a list or tuple"));
    };
    let (list, count) = match &heap.object(token.id)?.data {
        Data::ListSlice { .. } => (true, 0),
        Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } if tag == "$list" || tag == "$tuple" => {
            tagged_boundary(tag, *ordinary_fields, fields.len())?;
            (tag == "$list", fields.len())
        }
        _ => return Err(unsupported("array observer requires a list or tuple")),
    };
    if list {
        return heap.dup(value);
    }
    // Retaining and validating the new fields each visit the tuple's slots.
    // Pay before either pass; cleanup of accepted tokens never needs fuel.
    super::charge(fuel, count.saturating_mul(2))?;
    let mut fields = Vec::with_capacity(count);
    for index in 0..count {
        match heap.project(value, index) {
            Ok(field) => fields.push(field),
            Err(error) => {
                heap.drop_values(fields)?;
                return Err(error);
            }
        }
    }
    heap.construct("$list", fields)
}

/// Operands stay rooted by the caller until either result delivery or unwind.
fn unsafe_int(kind: Intrinsic, left: &Value, right: &Value) -> Result<Value, Error> {
    let (Value::Int(left), Value::Int(right)) = (left, right) else {
        return Err(unsupported("Int division/remainder type mismatch"));
    };
    let remainder = kind == Intrinsic::IntUnsafeRem;
    if *right == 0 {
        return Err(Error::IntDivisionByZero { remainder });
    }
    let op = if remainder { Arith::Rem } else { Arith::Div };
    Ok(Value::Int(int_arith(op, *left, *right)))
}

fn unsupported(what: impl Into<String>) -> Error {
    Error::UnsupportedPrimitive(what.into())
}

pub(super) fn not(value: &Value) -> Result<Value, Error> {
    match value {
        Value::Bool(value) => Ok(Value::Bool(!value)),
        _ => Err(Error::ExpectedBoolean),
    }
}

pub(super) fn neg(heap: &mut Heap, num: Num, value: &Value) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    match (num, value) {
        (Num::Int, Value::Int(value)) => Ok(Value::Int(value.wrapping_neg())),
        (Num::Int8, Value::Int8(value)) => Ok(Value::Int8(value.wrapping_neg())),
        (Num::Int16, Value::Int16(value)) => Ok(Value::Int16(value.wrapping_neg())),
        (Num::Int64, Value::Int64(value)) => Ok(Value::Int64(value.wrapping_neg())),
        (Num::UInt8, Value::UInt8(value)) => Ok(Value::UInt8(value.wrapping_neg())),
        (Num::UInt16, Value::UInt16(value)) => Ok(Value::UInt16(value.wrapping_neg())),
        (Num::UInt32, Value::UInt32(value)) => Ok(Value::UInt32(value.wrapping_neg())),
        (Num::UInt64, Value::UInt64(value)) => Ok(Value::UInt64(value.wrapping_neg())),
        (Num::Float, Value::Float(value)) => Ok(Value::Float(-value)),
        (Num::Float32, Value::Float32(value)) => Ok(Value::Float32(-value)),
        (Num::BigInt, _) => {
            let text = bigint_text(heap.bigint(value)?)?;
            let negative = if let Some(positive) = text.strip_prefix('-') {
                positive.to_owned()
            } else if text == "0" {
                text
            } else {
                format!("-{text}")
            };
            heap.literal(&Lit::BigInt(negative))
        }
        _ => Err(unsupported("numeric negation type mismatch")),
    }
}

/// Inputs remain rooted in the machine until success or error cleanup.
pub(super) fn string_predicate(
    heap: &Heap,
    kind: Intrinsic,
    needle: &Value,
    subject: &Value,
) -> Result<Value, Error> {
    heap.admit_ordinary(needle)?;
    heap.admit_ordinary(subject)?;
    let needle = heap.utf16_units(needle)?;
    let subject = heap.utf16_units(subject)?;
    let matched = match kind {
        Intrinsic::StringStartsWith => subject.starts_with(&needle),
        Intrinsic::StringIncludes => {
            needle.is_empty()
                || subject
                    .windows(needle.len())
                    .any(|w| w == needle.as_slice())
        }
        _ => return Err(unsupported("not a string predicate")),
    };
    Ok(Value::Bool(matched))
}

pub(super) fn show_value(heap: &Heap, value: &Value) -> Result<String, Error> {
    match value {
        Value::Unit => Ok("undefined".into()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Int(value) => Ok(value.to_string()),
        Value::Int8(value) => Ok(value.to_string()),
        Value::Int16(value) => Ok(value.to_string()),
        Value::Int64(value) => Ok(value.to_string()),
        Value::UInt8(value) => Ok(value.to_string()),
        Value::UInt16(value) => Ok(value.to_string()),
        Value::UInt32(value) => Ok(value.to_string()),
        Value::UInt64(value) => Ok(value.to_string()),
        Value::Float(value) => Ok(js_number(*value)),
        Value::Float32(value) => Ok(js_number(f64::from(*value))),
        Value::Object(token) => match &heap.object(token.id)?.data {
            Data::String(text) => Ok(json_string(text)),
            Data::Utf16(units) => Ok(show_utf16_string(units)),
            Data::Bytes(_) => Err(unsupported("primitive show of bytes")),
            Data::JsonNull => Ok("null".into()),
            Data::BigDecimal { value, scale } => Ok(decimal_ops::format_decimal(value, *scale)),
            Data::RegExp { pattern, flags, .. } => Ok(format!("/{pattern}/{flags}")),
            Data::Duration(d) => Ok(duration_ops::format_duration(d)),
            Data::BigInt(text) => Ok(format!("{}n", bigint_text(text)?)),
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                if tag.starts_with('$') {
                    return Err(unsupported("primitive show of collection"));
                }
                Ok(tag.rsplit('.').next().unwrap_or(tag).to_owned())
            }
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::CauseReason(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::ListSlice { .. }
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Closure { .. }
            | Data::Task { .. }
            | Data::Continuation { .. }
            | Data::Cell { .. }
            | Data::Var { .. }
            | Data::Lazy { .. }
            | Data::Arena { .. }
            | Data::Seq(_)
            | Data::Iterator { .. }
            | Data::Label(_)
            | Data::Exit(_)
            | Data::DateTime(_)
            | Data::TimeZone(_)
            | Data::Cron(_)
            | Data::Graph(_)
            | Data::GraphWalker(_)
            | Data::Context(_)
            | Data::ContextRef { .. }
            | Data::FiberHandle { .. }
            | Data::Deferred { .. }
            | Data::MemoMap { .. }
            | Data::FiberGroup { .. }
            | Data::MaskRestore { .. }
            | Data::MaskedTask { .. }
            | Data::CachedRunTask { .. }
            | Data::CachedInvalidateTask { .. }
            | Data::ExitTask(_)
            | Data::CapturedEnv { .. }
            | Data::ForeignHandle { .. }
            | Data::ForeignCycle { .. } => Err(unsupported(
                "primitive show of record or executable/opaque storage",
            )),
            Data::ReuseToken { .. } => Err(Error::ReuseTokenEscape),
        },
        Value::Cell(_) | Value::Var(_) | Value::Lazy(_) | Value::Iterator(_) => {
            Err(unsupported("primitive show of cell"))
        }
        Value::Owner(_) | Value::Arena(_) | Value::ArenaView(_) => {
            Err(unsupported("primitive show of arena capability"))
        }
        Value::ReuseNull => Err(Error::ReuseTokenEscape),
    }
}

fn show_utf16_string(units: &[u16]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(units.len() + 2);
    out.push('"');
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xD800..=0xDBFF).contains(&u)
            && i + 1 < units.len()
            && (0xDC00..=0xDFFF).contains(&units[i + 1])
        {
            let cp = 0x10000 + ((u32::from(u) - 0xD800) << 10) + (u32::from(units[i + 1]) - 0xDC00);
            if let Some(ch) = char::from_u32(cp) {
                let quoted = json_string(&ch.to_string());
                out.push_str(&quoted[1..quoted.len() - 1]);
            }
            i += 2;
        } else if (0xD800..=0xDFFF).contains(&u) {
            let _ = write!(out, "\\u{u:04x}");
            i += 1;
        } else {
            if let Some(ch) = char::from_u32(u32::from(u)) {
                let quoted = json_string(&ch.to_string());
                out.push_str(&quoted[1..quoted.len() - 1]);
            }
            i += 1;
        }
    }
    out.push('"');
    out
}

pub(super) fn intrinsic(
    heap: &mut Heap,
    kind: Intrinsic,
    value: &Value,
    stdout: &mut String,
) -> Result<Value, Error> {
    intrinsic_with_host(heap, HostMode::Deterministic, kind, value, stdout)
}

pub(super) fn intrinsic_with_host(
    heap: &mut Heap,
    host_mode: HostMode,
    kind: Intrinsic,
    value: &Value,
    stdout: &mut String,
) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    match kind {
        Intrinsic::PrintLine => {
            stdout.push_str(&heap.string_lossy(value)?);
            stdout.push('\n');
            Ok(Value::Unit)
        }
        Intrinsic::Show => {
            let text = show_value(heap, value)?;
            heap.literal(&Lit::String(text))
        }
        Intrinsic::Host(primitive) => {
            host::run_with_mode(heap, host_mode, primitive, value, stdout)
        }
        _ => Err(Error::NeedsProgram),
    }
}
include!("primitives/arith.rs");
include!("primitives/operators.rs");

#[cfg(test)]
#[path = "primitives_tests.rs"]
mod array_observer_tests;
