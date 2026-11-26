//! Pure metered `Arbitrary` formatting, constraints, and linked-type constructors (Stage 4D).

use std::fmt::Write as _;

use flick_core::ir::Lit;
use flick_core::opt::{js_number, json_string};

use super::arbitrary_ops::expect_num;
use super::bigint_ops::parse_operand;
use super::cause_pretty::wire_tag;
use super::datetime_tzif::zone_named;
use super::duration_ops::{from_millis, from_nanos, is_negative};
use super::list_ops::slice_reps;
use super::{Data, DateTimeData, DurationData, Error, Heap, TimeZoneData, Value, charge};

#[path = "arbitrary_fmt/constraints.rs"]
mod constraints;

pub(super) use constraints::{extract_constraint, merge_constraint};

pub(super) fn js_string_inner(heap: &Heap, val: &Value) -> Result<String, Error> {
    match val {
        Value::Unit => Ok("undefined".into()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(js_number(*f)),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::String(s) | Data::BigInt(s) => Ok(s.clone()),
            Data::Utf16(u) => Ok(String::from_utf16_lossy(u)),
            Data::JsonNull => Ok("null".into()),
            _ => super::show_value(heap, val),
        },
        _ => Ok(String::new()),
    }
}

pub(super) fn js_string_val(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let s = js_string_inner(heap, val)?;
    heap.literal(&Lit::String(s))
}

pub(super) fn quote_json_op(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let units = heap.utf16_units(val)?;
    heap.literal(&Lit::String(quote_utf16(&units)))
}

fn quote_utf16(units: &[u16]) -> String {
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
                let q = json_string(&ch.to_string());
                out.push_str(&q[1..q.len() - 1]);
            }
            i += 2;
        } else if (0xD800..=0xDFFF).contains(&u) {
            let _ = write!(out, "\\u{u:04x}");
            i += 1;
        } else {
            if let Some(ch) = char::from_u32(u32::from(u)) {
                let q = json_string(&ch.to_string());
                out.push_str(&q[1..q.len() - 1]);
            }
            i += 1;
        }
    }
    out.push('"');
    out
}

pub(super) fn format_spaced_op(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let text = format_spaced(heap, val, 2, 0, fuel)?;
    heap.literal(&Lit::String(text))
}

fn format_entries(
    heap: &Heap,
    keys: Option<&[String]>,
    vals: &[Value],
    space: usize,
    depth: usize,
    fuel: &mut u64,
) -> Result<String, Error> {
    let (open, close) = if keys.is_some() {
        ("{", "}")
    } else {
        ("[", "]")
    };
    let mut out = String::from(open);
    let multiline = space > 0 && vals.len() > 1;
    let gap = " ".repeat(space);
    for (i, v) in vals.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if multiline {
            out.push('\n');
            out.push_str(&gap.repeat(depth + 1));
        }
        if let Some(ks) = keys {
            out.push_str(&json_string(&ks[i]));
            out.push(':');
            if multiline {
                out.push(' ');
            }
        }
        let next_d = if multiline { depth + 1 } else { depth };
        out.push_str(&format_spaced(heap, v, space, next_d, fuel)?);
    }
    if multiline {
        out.push('\n');
        out.push_str(&gap.repeat(depth));
    }
    out.push_str(close);
    Ok(out)
}

fn format_duration_js(d: &DurationData) -> String {
    match d {
        DurationData::Infinite => "Infinity".into(),
        DurationData::NegativeInfinite => "-Infinity".into(),
        DurationData::Nanos(n) => format!("{n} nanos"),
        DurationData::Millis(ms) => format!("{} millis", js_number(*ms)),
    }
}

fn format_spaced(
    heap: &Heap,
    val: &Value,
    space: usize,
    depth: usize,
    fuel: &mut u64,
) -> Result<String, Error> {
    charge(fuel, 1)?;
    match val {
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(js_number(*f)),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::String(s) => Ok(json_string(s)),
            Data::Utf16(u) => Ok(quote_utf16(u)),
            Data::JsonNull => Ok("null".into()),
            Data::BigInt(s) => Ok(format!("{s}n")),
            Data::Duration(d) => Ok(format_duration_js(d)),
            Data::ListSlice { .. } => {
                let elems = slice_reps(heap, val)?;
                format_entries(heap, None, &elems, space, depth, fuel)
            }
            Data::Record(entries) => {
                let keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();
                let vals: Vec<Value> = entries.iter().map(|(_, v)| v.representation()).collect();
                format_entries(heap, Some(&keys), &vals, space, depth, fuel)
            }
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
                ..
            } => {
                if tag == "$list" || tag == "$tuple" {
                    let vals: Vec<Value> = fields[..*ordinary_fields]
                        .iter()
                        .map(Value::representation)
                        .collect();
                    return format_entries(heap, None, &vals, space, depth, fuel);
                }
                if tag == "Redacted.Redacted.$Redacted" {
                    return Ok("<redacted>".into());
                }
                if tag == "Prelude.Option.None" {
                    return Ok("none()".into());
                }
                if tag == "Prelude.Option.Some" && *ordinary_fields == 1 {
                    let inner = format_spaced(heap, &fields[0], 0, 0, fuel)?;
                    return Ok(format!("some({inner})"));
                }
                if tag == "Prelude.Result.Ok" && *ordinary_fields == 1 {
                    let inner = format_spaced(heap, &fields[0], 0, 0, fuel)?;
                    return Ok(format!("success({inner})"));
                }
                if tag == "Prelude.Result.Err" && *ordinary_fields == 1 {
                    let inner = format_spaced(heap, &fields[0], 0, 0, fuel)?;
                    return Ok(format!("failure({inner})"));
                }
                let mut keys = vec!["_tag".to_owned()];
                let mut rendered_tag_only = Vec::new();
                for (i, f) in fields[..*ordinary_fields].iter().enumerate() {
                    keys.push(format!("_{i}"));
                    rendered_tag_only.push(f.representation());
                }
                // Format with `_tag` string inline without allocating on heap.
                let wtag = wire_tag(tag);
                let total = 1 + rendered_tag_only.len();
                let multiline = space > 0 && total > 1;
                let gap = " ".repeat(space);
                let mut out = String::from("{");
                if multiline {
                    out.push('\n');
                    out.push_str(&gap.repeat(depth + 1));
                    let _ = write!(out, "\"_tag\": {}", json_string(&wtag));
                } else {
                    let _ = write!(out, "\"_tag\":{}", json_string(&wtag));
                }
                for (i, fv) in rendered_tag_only.iter().enumerate() {
                    out.push(',');
                    if multiline {
                        out.push('\n');
                        out.push_str(&gap.repeat(depth + 1));
                        let _ = write!(out, "\"_{i}\": ");
                    } else {
                        let _ = write!(out, "\"_{i}\":");
                    }
                    let next_d = if multiline { depth + 1 } else { depth };
                    out.push_str(&format_spaced(heap, fv, space, next_d, fuel)?);
                }
                if multiline {
                    out.push('\n');
                    out.push_str(&gap.repeat(depth));
                }
                out.push('}');
                Ok(out)
            }
            _ => super::show_value(heap, val),
        },
        _ => Ok("undefined".into()),
    }
}

pub(super) fn make_duration(
    heap: &mut Heap,
    tag_val: &Value,
    payload: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let tag = heap.string_lossy(tag_val)?;
    let d = match tag.as_str() {
        "Infinity" => DurationData::Infinite,
        "NegativeInfinity" => DurationData::NegativeInfinite,
        "Nanos" => from_nanos(parse_operand(heap, payload, fuel)?),
        "Millis" => from_millis(expect_num(payload)?),
        _ => return Err(Error::RangeError(format!("unknown duration tag: {tag}"))),
    };
    Ok(heap.allocate(Data::Duration(d)))
}

pub(super) fn is_non_negative_duration(heap: &Heap, val: &Value) -> Result<Value, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Duration"));
    };
    let Data::Duration(d) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Duration"));
    };
    Ok(Value::Bool(!is_negative(d)))
}

fn zone_of_val(heap: &Heap, val: &Value) -> Result<TimeZoneData, Error> {
    match val {
        Value::Float(f) => Ok(TimeZoneData::Offset(*f)),
        Value::Int(i) => Ok(TimeZoneData::Offset(f64::from(*i))),
        _ => {
            let name = heap.string_lossy(val)?;
            zone_named(&name).ok_or_else(|| Error::RangeError(format!("Invalid time zone: {name}")))
        }
    }
}

pub(super) fn make_zone(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let tz = zone_of_val(heap, val)?;
    Ok(heap.allocate(Data::TimeZone(tz)))
}

pub(super) fn make_zoned_datetime(
    heap: &mut Heap,
    ms_val: &Value,
    zone_val: &Value,
) -> Result<Value, Error> {
    let ms = expect_num(ms_val)?;
    let tz = zone_of_val(heap, zone_val)?;
    Ok(heap.allocate(Data::DateTime(DateTimeData { ms, zone: Some(tz) })))
}
