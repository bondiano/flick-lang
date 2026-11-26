//! Node-level builders for the flat-node wire decoder.

use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use num_bigint::BigInt;
use regex_automata::meta::Regex;
use serde_json::Value as Json;

use crate::tier0::causes::{CauseData, StoredReason};
use crate::tier0::{
    Data, DateTimeData, DefectSpec, DurationData, Error, ExitData, Heap, OwnerHandle, TimeZoneData,
    Value,
};

use super::decode::{take_child, wire_idx};

pub(super) fn build_decoded_node(
    heap: &mut Heap,
    execution: usize,
    kind: &str,
    node: &[Json],
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
) -> Result<Value, Error> {
    if let Some(val) = decode_scalar_node(heap, execution, kind, node)? {
        return Ok(val);
    }
    decode_compound_node(heap, kind, node, uses, resolved)
}

fn decode_scalar_node(
    heap: &mut Heap,
    execution: usize,
    kind: &str,
    node: &[Json],
) -> Result<Option<Value>, Error> {
    let val = match kind {
        "u" => Value::Unit,
        "null" => heap.allocate(Data::JsonNull),
        "b" => Value::Bool(node.get(1).and_then(Json::as_bool).unwrap_or(false)),
        "i" => {
            let n = node
                .get(1)
                .and_then(Json::as_i64)
                .ok_or_else(|| Error::HostBridge("invalid int wire node".into()))?;
            #[expect(clippy::cast_possible_truncation, reason = "JS 32-bit int")]
            Value::Int(n as i32)
        }
        "f" => {
            let f = node
                .get(1)
                .and_then(Json::as_f64)
                .ok_or_else(|| Error::HostBridge("invalid float wire node".into()))?;
            Value::Float(f)
        }
        "fs" => Value::Float(match node.get(1).and_then(Json::as_str) {
            Some("NaN") => f64::NAN,
            Some("Infinity") => f64::INFINITY,
            Some("-Infinity") => f64::NEG_INFINITY,
            _ => -0.0,
        }),
        "n" => {
            let text = node
                .get(1)
                .and_then(Json::as_str)
                .ok_or_else(|| Error::HostBridge("invalid bigint wire node".into()))?;
            heap.allocate(Data::BigInt(text.to_owned()))
        }
        "s" => {
            let text = node
                .get(1)
                .and_then(Json::as_str)
                .ok_or_else(|| Error::HostBridge("invalid string wire node".into()))?;
            heap.allocate(Data::String(text.to_owned()))
        }
        "s16" => decode_utf16_node(heap, node)?,
        "bytes" => {
            let b64 = node
                .get(1)
                .and_then(Json::as_str)
                .ok_or_else(|| Error::HostBridge("invalid bytes wire node".into()))?;
            let bytes = BASE64
                .decode(b64)
                .map_err(|e| Error::HostBridge(format!("invalid base64 bytes: {e}")))?;
            heap.allocate(Data::Bytes(bytes))
        }
        "dec" => {
            let text = node
                .get(1)
                .and_then(Json::as_str)
                .ok_or_else(|| Error::HostBridge("invalid decimal wire node".into()))?;
            let (value, scale) = parse_decimal_wire(text)?;
            heap.allocate(Data::BigDecimal { value, scale })
        }
        "dur" => decode_duration_node(heap, node)?,
        "dt" => decode_datetime_node(heap, node)?,
        "re" => decode_regexp_node(heap, node)?,
        "handle" => {
            let host_id = node
                .get(1)
                .and_then(Json::as_u64)
                .ok_or_else(|| Error::HostBridge("handle missing host_id".into()))?;
            heap.allocate(Data::ForeignHandle { execution, host_id })
        }
        "cycle" => {
            let host_id = node
                .get(1)
                .and_then(Json::as_u64)
                .ok_or_else(|| Error::HostBridge("cycle missing host_id".into()))?;
            heap.allocate(Data::ForeignCycle { execution, host_id })
        }
        "owner" => {
            let index = node
                .get(1)
                .and_then(Json::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| Error::HostBridge("owner missing index".into()))?;
            Value::Owner(OwnerHandle {
                heap: heap.identity,
                execution,
                index,
            })
        }
        "stag" => {
            let key = node
                .get(1)
                .and_then(Json::as_str)
                .ok_or_else(|| Error::HostBridge("stag missing key".into()))?;
            heap.service_tag(key)?
        }
        _ => return Ok(None),
    };
    Ok(Some(val))
}

fn decode_utf16_node(heap: &mut Heap, node: &[Json]) -> Result<Value, Error> {
    let arr = node
        .get(1)
        .and_then(Json::as_array)
        .ok_or_else(|| Error::HostBridge("invalid s16 wire node".into()))?;
    let mut units = Vec::with_capacity(arr.len());
    for u in arr {
        let code = u
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(|| Error::HostBridge("invalid utf16 code unit".into()))?;
        units.push(code);
    }
    Ok(heap.alloc_utf16(units))
}

fn decode_duration_node(heap: &mut Heap, node: &[Json]) -> Result<Value, Error> {
    let d = match node.get(1).and_then(Json::as_str) {
        Some("ms") => DurationData::Millis(node.get(2).and_then(Json::as_f64).unwrap_or(0.0)),
        Some("ns") => {
            let s = node.get(2).and_then(Json::as_str).unwrap_or("0");
            let ns = s
                .parse::<BigInt>()
                .map_err(|e| Error::HostBridge(format!("invalid duration nanos: {e}")))?;
            DurationData::Nanos(ns)
        }
        Some("inf") => DurationData::Infinite,
        _ => DurationData::Millis(0.0),
    };
    Ok(heap.allocate(Data::Duration(d)))
}

fn decode_datetime_node(heap: &mut Heap, node: &[Json]) -> Result<Value, Error> {
    let ms = node
        .get(1)
        .and_then(Json::as_f64)
        .ok_or_else(|| Error::HostBridge("invalid datetime wire node".into()))?;
    let zone = match node.get(2).and_then(Json::as_array) {
        Some(arr) if arr.first().and_then(Json::as_str) == Some("offset") => {
            arr.get(1).and_then(Json::as_f64).map(TimeZoneData::Offset)
        }
        Some(arr) if arr.first().and_then(Json::as_str) == Some("named") => arr
            .get(1)
            .and_then(Json::as_str)
            .map(|id| TimeZoneData::Named {
                id: id.to_owned(),
                fixed_seconds: None,
                is_local: false,
            }),
        _ => None,
    };
    Ok(heap.allocate(Data::DateTime(DateTimeData { ms, zone })))
}

fn decode_regexp_node(heap: &mut Heap, node: &[Json]) -> Result<Value, Error> {
    let pattern = node
        .get(1)
        .and_then(Json::as_str)
        .ok_or_else(|| Error::HostBridge("invalid regexp pattern".into()))?
        .to_owned();
    let flags = node.get(2).and_then(Json::as_str).unwrap_or("").to_owned();
    let mut inline = String::new();
    for flag in ['i', 'm', 's'] {
        if flags.contains(flag) {
            inline.push(flag);
        }
    }
    let src = if inline.is_empty() {
        pattern.clone()
    } else {
        format!("(?{inline}){pattern}")
    };
    let compiled =
        Regex::new(&src).map_err(|e| Error::HostBridge(format!("compile regexp: {e}")))?;
    let global = flags.contains('g');
    Ok(heap.allocate(Data::RegExp {
        pattern,
        flags,
        global,
        compiled,
    }))
}

fn decode_compound_node(
    heap: &mut Heap,
    kind: &str,
    node: &[Json],
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
) -> Result<Value, Error> {
    let len = resolved.len();
    match kind {
        "list" | "tuple" => {
            let items = node
                .get(1)
                .and_then(Json::as_array)
                .ok_or_else(|| Error::HostBridge("list/tuple missing items".into()))?;
            let values = take_child_vec(heap, items, len, uses, resolved)?;
            let tag = if kind == "list" { "$list" } else { "$tuple" };
            heap.construct(tag, values)
        }
        "rec" | "dict" => decode_record_node(heap, node, len, uses, resolved),
        "tag" => {
            let full_tag = node
                .get(1)
                .and_then(Json::as_str)
                .ok_or_else(|| Error::HostBridge("tag missing name".into()))?;
            let field_ids = node
                .get(2)
                .and_then(Json::as_array)
                .ok_or_else(|| Error::HostBridge("tag missing fields".into()))?;
            let fields = take_child_vec(heap, field_ids, len, uses, resolved)?;
            heap.construct(full_tag, fields)
        }
        "exit_ok" => {
            let cidx = wire_idx(
                node.get(1)
                    .ok_or_else(|| Error::HostBridge("exit_ok missing val".into()))?,
                len,
            )?;
            let inner = take_child(heap, cidx, uses, resolved)?;
            Ok(heap.allocate(Data::Exit(ExitData::Success(inner))))
        }
        "exit_fail" => {
            let cidx = wire_idx(
                node.get(1)
                    .ok_or_else(|| Error::HostBridge("exit_fail missing cause".into()))?,
                len,
            )?;
            let inner = take_child(heap, cidx, uses, resolved)?;
            Ok(heap.allocate(Data::Exit(ExitData::Failure(inner))))
        }
        "cause" => decode_cause_node(heap, node, len, uses, resolved),
        other => Err(Error::HostBridge(format!(
            "unknown wire node kind: {other}"
        ))),
    }
}

fn take_child_vec(
    heap: &mut Heap,
    items: &[Json],
    len: usize,
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let cidx = wire_idx(item, len)?;
        match take_child(heap, cidx, uses, resolved) {
            Ok(v) => values.push(v),
            Err(e) => {
                let _ = heap.drop_values(values);
                return Err(e);
            }
        }
    }
    Ok(values)
}

fn decode_record_node(
    heap: &mut Heap,
    node: &[Json],
    len: usize,
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
) -> Result<Value, Error> {
    let pairs = node
        .get(1)
        .and_then(Json::as_array)
        .ok_or_else(|| Error::HostBridge("rec missing pairs".into()))?;
    let mut seen = BTreeSet::new();
    let mut fields = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let arr = pair
            .as_array()
            .ok_or_else(|| Error::HostBridge("rec entry not array".into()))?;
        let key = arr
            .first()
            .and_then(Json::as_str)
            .ok_or_else(|| Error::HostBridge("rec key not string".into()))?
            .to_owned();
        let cidx = wire_idx(
            arr.get(1)
                .ok_or_else(|| Error::HostBridge("rec entry missing val".into()))?,
            len,
        )?;
        let val = match take_child(heap, cidx, uses, resolved) {
            Ok(v) => v,
            Err(e) => {
                let _ = heap.drop_values(fields.into_iter().map(|(_, v)| v).collect());
                return Err(e);
            }
        };
        if seen.insert(key.clone()) {
            fields.push((key, val));
        } else {
            let _ = heap.drop_value(val);
        }
    }
    heap.record(fields)
}

fn decode_cause_node(
    heap: &mut Heap,
    node: &[Json],
    len: usize,
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
) -> Result<Value, Error> {
    let reasons_arr = node
        .get(1)
        .and_then(Json::as_array)
        .ok_or_else(|| Error::HostBridge("cause missing reasons".into()))?;
    let mut stored = Vec::with_capacity(reasons_arr.len());
    for r in reasons_arr {
        let arr = r
            .as_array()
            .ok_or_else(|| Error::HostBridge("cause reason not array".into()))?;
        match arr.first().and_then(Json::as_str) {
            Some("fail") => {
                let cidx = wire_idx(
                    arr.get(1)
                        .ok_or_else(|| Error::HostBridge("fail reason missing val".into()))?,
                    len,
                )?;
                match take_child(heap, cidx, uses, resolved) {
                    Ok(v) => stored.push(StoredReason::Fail(v)),
                    Err(e) => {
                        let _ = heap.drop_values(
                            stored
                                .into_iter()
                                .filter_map(StoredReason::into_value)
                                .collect(),
                        );
                        return Err(e);
                    }
                }
            }
            Some("die") => {
                let msg = arr.get(1).and_then(Json::as_str).unwrap_or("host defect");
                let defect = heap.allocate(Data::Defect(DefectSpec::custom(msg)));
                stored.push(StoredReason::Die(defect));
            }
            Some("die_val") => {
                let cidx = wire_idx(
                    arr.get(1)
                        .ok_or_else(|| Error::HostBridge("die_val missing val".into()))?,
                    len,
                )?;
                match take_child(heap, cidx, uses, resolved) {
                    Ok(v) => stored.push(StoredReason::Die(v)),
                    Err(e) => {
                        let _ = heap.drop_values(
                            stored
                                .into_iter()
                                .filter_map(StoredReason::into_value)
                                .collect(),
                        );
                        return Err(e);
                    }
                }
            }
            Some("interrupt") => {
                let fid = arr
                    .get(1)
                    .and_then(Json::as_u64)
                    .and_then(|v| usize::try_from(v).ok());
                stored.push(StoredReason::Interrupt { fiber_id: fid });
            }
            _ => return Err(Error::HostBridge("unknown cause reason".into())),
        }
    }
    Ok(heap.allocate(Data::Cause(CauseData::plain(stored))))
}

fn parse_decimal_wire(text: &str) -> Result<(BigInt, i64), Error> {
    let s = text.trim();
    let (mantissa, exp): (&str, i64) = if let Some(idx) = s.find(['e', 'E']) {
        let e = s[idx + 1..]
            .parse::<i64>()
            .map_err(|_| Error::HostBridge(format!("invalid decimal exponent: {text}")))?;
        (&s[..idx], e)
    } else {
        (s, 0)
    };
    let (digits, frac_len) = if let Some(dot) = mantissa.find('.') {
        let mut d = String::with_capacity(mantissa.len() - 1);
        d.push_str(&mantissa[..dot]);
        d.push_str(&mantissa[dot + 1..]);
        let fl = i64::try_from(mantissa.len() - dot - 1).unwrap_or(0);
        (d, fl)
    } else {
        (mantissa.to_owned(), 0)
    };
    let value = digits
        .parse::<BigInt>()
        .map_err(|_| Error::HostBridge(format!("invalid decimal digits: {text}")))?;
    Ok((value, frac_len - exp))
}
