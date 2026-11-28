//! Tier-neutral standard primitive dispatch.
//! Value operations borrow their inputs; the calling VM owns and releases them.
use std::cmp::Ordering;
use std::collections::BTreeSet;

use flick_core::ir::memory::Intrinsic;
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{
    Arith, Num, int_arith, int8_arith, int16_arith, int64_arith, uint8_arith, uint16_arith,
    uint32_arith, uint64_arith,
};
use flick_core::opt::{js_number, json_string};

use crate::stdlib::hooks::{Fuel, VmHooks, charge};
use crate::stdlib::host;
use crate::tier0::causes::StoredReason;
use crate::tier0::{
    CronData, Data, DateTimeData, DurationData, Error, ExitData, GraphData, GraphEdge, GraphNode,
    GraphWalkerData, Heap, ObjectId, ObjectToken, TimeZoneData, Value, tagged_boundary,
};

mod arbitrary_fmt;
mod arbitrary_ops;
mod arbitrary_shrink;
mod bigint;
mod bigint_conversion;
mod bigint_norm;
mod bigint_ops;
mod bytes_ops;
mod cause_ops;
mod cause_pretty;
mod check_eval;
mod check_ops;
mod config_ops;
mod config_url;
mod cron_ops;
mod cron_step;
mod crypto_ops;
mod datetime_date;
mod datetime_format;
mod datetime_ops;
mod datetime_parse;
mod datetime_parse_scan;
mod datetime_parts;
mod datetime_tzif;
mod datetime_tzif_bin;
mod debug_ops;
mod decimal_ops;
mod decimal_round;
mod duration_ops;
mod fixed_width_ops;
mod float_ops;
mod gen_ops;
mod graph_algos;
mod graph_allpaths;
mod graph_build;
mod graph_combine;
mod graph_conn;
mod graph_flow;
mod graph_json;
mod graph_ops;
mod graph_paths;
mod graph_query;
mod graph_transform;
mod graph_walk;
mod int_ops;
mod isaac_ops;
mod json_ops;
mod json_patch_ops;
mod json_pointer_ops;
mod json_schema_conv;
mod json_schema_dialect;
mod json_schema_export;
mod json_schema_jval;
mod json_schema_to;
mod list_ops;
mod list_ops_extra;
mod numeric;
mod outcome_ops;
mod path_ops;
mod path_url;
mod record_ops;
mod regexp_ops;
mod scalar;
mod schema_data;
mod schema_fln_encode;
mod schema_fln_leaves;
mod schema_leaves;
mod schema_leaves_encode;
mod schema_ops;
mod schema_route_ops;
mod schema_std_leaves;
mod string_case;
mod string_ops;

pub(crate) use bigint_norm::bigint_pattern_eq;
use bigint_norm::{bigint_order, bigint_text, normalize_bigint};
pub(crate) use cause_ops::{
    concat_causes, first_defect_message, join_causes, strip_fail_and_add_interrupt,
};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
pub(crate) use decimal_ops::format_decimal;
pub use float_ops::{parse_float, parse_number};
pub(crate) use isaac_ops::{
    IsaacRng, describe_cause_report, duration_sleep_nanos, fails_to_dies_cause,
    without_interrupts_cause,
};
pub(crate) use outcome_ops::describe_defect;

/// Reports whether `kind` is handled by the tier-neutral pure primitive dispatcher.
pub fn is_metered_primitive(kind: Intrinsic) -> bool {
    matches!(
        kind,
        Intrinsic::ArrayToList
            | Intrinsic::ListRange
            | Intrinsic::PrimitiveCompare
            | Intrinsic::PrimitiveHash
            | Intrinsic::IntUnsafeDiv
            | Intrinsic::IntUnsafeRem
            | Intrinsic::NumberToFloat
            | Intrinsic::ToBigInt
            | Intrinsic::FloatMin
            | Intrinsic::FloatMax
            | Intrinsic::StringLength
            | Intrinsic::RecordGet
            | Intrinsic::RecordExtend
            | Intrinsic::RecordRemove
            | Intrinsic::RecordProject
            | Intrinsic::RecordMergeDisjoint
            | Intrinsic::RecordLabels
            | Intrinsic::ExitIsSuccess
            | Intrinsic::ExitIsFailure
            | Intrinsic::ExitHasInterrupts
            | Intrinsic::Int(_)
            | Intrinsic::FixedWidth(_)
            | Intrinsic::Float(_)
            | Intrinsic::BigInt(_)
            | Intrinsic::String(_)
            | Intrinsic::List(_)
            | Intrinsic::Record(_)
            | Intrinsic::Debug(_)
            | Intrinsic::Bytes(_)
            | Intrinsic::Hash(_)
            | Intrinsic::Json(_)
            | Intrinsic::BigDecimal(_)
            | Intrinsic::RegExp(_)
            | Intrinsic::Duration(_)
            | Intrinsic::Outcome(_)
            | Intrinsic::Gen(_)
            | Intrinsic::Schema(_)
            | Intrinsic::JsonSchema(_)
            | Intrinsic::Path(_)
            | Intrinsic::DateTime(_)
            | Intrinsic::Cron(_)
            | Intrinsic::Graph(_)
            | Intrinsic::Crypto(_)
            | Intrinsic::Config(_)
            | Intrinsic::Arbitrary(_)
    )
}

pub(crate) fn metered_primitive(
    heap: &mut Heap,
    kind: Intrinsic,
    arguments: &[Value],
    fuel: Fuel<'_>,
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
            charge(fuel, units.len())?;
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
pub(crate) fn array_to_list(
    heap: &mut Heap,
    value: &Value,
    fuel: Fuel<'_>,
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
            ..
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
    charge(fuel, count.saturating_mul(2))?;
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

pub(crate) fn not(value: &Value) -> Result<Value, Error> {
    match value {
        Value::Bool(value) => Ok(Value::Bool(!value)),
        _ => Err(Error::ExpectedBoolean),
    }
}

pub(crate) fn neg(heap: &mut Heap, num: Num, value: &Value) -> Result<Value, Error> {
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

/// Inputs remain rooted in the caller until success or error cleanup.
pub(crate) fn string_predicate(
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

include!("primitives/show.rs");

pub(crate) fn intrinsic_with_hooks(
    heap: &mut Heap,
    kind: Intrinsic,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    match kind {
        Intrinsic::PrintLine => {
            hooks.write_stdout(&heap.string_lossy(value)?);
            hooks.write_stdout("\n");
            Ok(Value::Unit)
        }
        Intrinsic::Show => {
            let text = show_value(heap, value)?;
            heap.literal(&Lit::String(text))
        }
        Intrinsic::Host(primitive) => host::run_with_hooks(heap, primitive, value, hooks),
        _ => Err(Error::NeedsProgram),
    }
}
include!("primitives/arith.rs");
include!("primitives/operators.rs");

#[cfg(test)]
mod tests;
