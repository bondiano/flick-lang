//! Counted literal/split codecs and `Route` URL parsing/formatting primitives.

use flick_core::ir::Lit;

use super::json_ops::stringify_value;
use super::json_pointer_ops::decode_uri_component;
use super::list_ops::slice_reps;
use super::record_ops::put_js_key;
use super::schema_leaves::{is_str_val, js_number_parse};
use super::schema_leaves_encode::hex_nibble;
use super::schema_ops::{expected_err, same_value};
use super::string_ops::js_trim;
use super::{Data, Error, Heap, Value, charge};

pub(super) fn literal_accepts(
    heap: &Heap,
    values_val: &Value,
    input: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, values_val)?;
    charge(fuel, reps.len())?;
    for v in &reps {
        if same_value(heap, v, input)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

pub(super) fn literal_decode(
    heap: &mut Heap,
    values_val: &Value,
    is_union: &Value,
    input: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, values_val)?;
    charge(fuel, reps.len())?;
    for v in &reps {
        if same_value(heap, v, input)? {
            let d = heap.dup(input)?;
            return heap.construct("Prelude.Result.Ok", vec![d]);
        }
    }
    if matches!(is_union, Value::Bool(true)) || reps.is_empty() {
        expected_err(heap, "Union", input)
    } else {
        let want = stringify_value(heap, &reps[0], fuel)?;
        expected_err(heap, &want, input)
    }
}

pub(super) fn split_decode(
    heap: &mut Heap,
    sep_val: &Value,
    input: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    if !is_str_val(heap, input)? {
        return expected_err(heap, "a string", input);
    }
    let text = heap.string_lossy(input)?;
    let sep = heap.string_lossy(sep_val)?;
    charge(fuel, text.len())?;
    if text.is_empty() {
        let empty = heap.construct("$list", vec![])?;
        return heap.construct("Prelude.Result.Ok", vec![empty]);
    }
    let parts: Vec<String> = if sep.is_empty() {
        text.chars().map(|c| c.to_string()).collect()
    } else {
        text.split(&sep).map(ToOwned::to_owned).collect()
    };
    let mut vals = Vec::with_capacity(parts.len());
    for p in parts {
        match heap.literal(&Lit::String(p)) {
            Ok(v) => vals.push(v),
            Err(err) => {
                heap.drop_values(vals)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", vals)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn split_encode(
    heap: &mut Heap,
    sep_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let sep = heap.string_lossy(sep_val)?;
    let reps = slice_reps(heap, val)?;
    charge(fuel, reps.len())?;
    let mut parts = Vec::with_capacity(reps.len());
    for r in &reps {
        parts.push(heap.string_lossy(r)?);
    }
    let s = heap.literal(&Lit::String(parts.join(&sep)))?;
    heap.construct("Prelude.Result.Ok", vec![s])
}

pub(super) fn split_kv_decode(
    heap: &mut Heap,
    sep_val: &Value,
    kv_val: &Value,
    input: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    if !is_str_val(heap, input)? {
        return expected_err(heap, "a string", input);
    }
    let text = heap.string_lossy(input)?;
    let sep = heap.string_lossy(sep_val)?;
    let kv = heap.string_lossy(kv_val)?;
    charge(fuel, text.len())?;
    let mut entries: Vec<(String, Value)> = Vec::new();
    let pairs: Vec<&str> = if sep.is_empty() {
        text.split("").filter(|s| !s.is_empty()).collect()
    } else {
        text.split(&sep).collect()
    };
    for pair in pairs {
        let parsed = if kv.is_empty() {
            let mut chars = pair.chars();
            let k = chars.next().map(|c| c.to_string()).unwrap_or_default();
            let v = chars.next().map(|c| c.to_string()).unwrap_or_default();
            (!k.is_empty() && !v.is_empty()).then_some((k, v))
        } else {
            pair.split_once(&kv)
                .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        };
        if let Some((k, v)) = parsed {
            let vv = match heap.literal(&Lit::String(v)) {
                Ok(x) => x,
                Err(err) => {
                    heap.drop_values(entries.into_iter().map(|(_, val)| val).collect())?;
                    return Err(err);
                }
            };
            if let Some(old) = put_js_key(&mut entries, k, vv) {
                heap.drop_value(old)?;
            }
        }
    }
    let mut out = Vec::with_capacity(entries.len());
    for (k, v) in entries {
        let kv_lit = match heap.literal(&Lit::String(k)) {
            Ok(x) => x,
            Err(err) => {
                heap.drop_value(v)?;
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        match heap.construct("$tuple", vec![kv_lit, v]) {
            Ok(t) => out.push(t),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", out)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn split_kv_encode(
    heap: &mut Heap,
    sep_val: &Value,
    kv_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let sep = heap.string_lossy(sep_val)?;
    let kv = heap.string_lossy(kv_val)?;
    let reps = slice_reps(heap, val)?;
    charge(fuel, reps.len())?;
    let mut parts = Vec::with_capacity(reps.len());
    for r in &reps {
        let Value::Object(tok) = r else {
            return Err(Error::PatternKind("tuple"));
        };
        let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
            return Err(Error::PatternKind("tuple"));
        };
        parts.push(format!(
            "{}{kv}{}",
            heap.string_lossy(&fields[0])?,
            heap.string_lossy(&fields[1])?
        ));
    }
    let s = heap.literal(&Lit::String(parts.join(&sep)))?;
    heap.construct("Prelude.Result.Ok", vec![s])
}

pub(super) fn encode_uri_component(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

fn form_encode_part(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'*' | b'-' | b'.' | b'_') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xf)]));
        }
    }
    out
}

fn form_decode_part(s: &str) -> String {
    let plus_space = s.replace('+', " ");
    let bytes = plus_space.as_bytes();
    let mut raw = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            raw.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        raw.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&raw).into_owned()
}

pub(super) fn route_format_url(
    heap: &mut Heap,
    parts_val: &Value,
    query_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let part_reps = slice_reps(heap, parts_val)?;
    let query_reps = slice_reps(heap, query_val)?;
    charge(fuel, part_reps.len() + query_reps.len())?;
    let mut out = String::new();
    for p in &part_reps {
        out.push('/');
        out.push_str(&encode_uri_component(&heap.string_lossy(p)?));
    }
    if out.is_empty() {
        out.push('/');
    }
    if !query_reps.is_empty() {
        out.push('?');
        for (i, q) in query_reps.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            let Value::Object(tok) = q else {
                return Err(Error::PatternKind("tuple"));
            };
            let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
                return Err(Error::PatternKind("tuple"));
            };
            out.push_str(&form_encode_part(&heap.string_lossy(&fields[0])?));
            out.push('=');
            out.push_str(&form_encode_part(&heap.string_lossy(&fields[1])?));
        }
    }
    heap.literal(&Lit::String(out))
}

pub(super) fn route_parse_url(
    heap: &mut Heap,
    text_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let raw = heap.string_lossy(text_val)?;
    charge(fuel, raw.len())?;
    let without_frag = raw
        .trim_matches(|c: char| c <= ' ')
        .split('#')
        .next()
        .unwrap_or("");
    let (before_query, query_str) = match without_frag.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (without_frag, None),
    };
    let path = if let Some(after) = before_query.split_once("://").map(|(_, r)| r) {
        after.find('/').map_or("/", |idx| &after[idx..])
    } else if let Some(after) = before_query.strip_prefix("//") {
        after.find('/').map_or("/", |idx| &after[idx..])
    } else {
        before_query
    };
    let normalized = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    let raw_parts: Vec<&str> = if normalized == "/" {
        Vec::new()
    } else {
        normalized[1..].split('/').collect()
    };
    let mut part_vals = Vec::with_capacity(raw_parts.len());
    for p in raw_parts {
        let decoded = decode_uri_component(p).unwrap_or_else(|| p.to_owned());
        match heap.literal(&Lit::String(decoded)) {
            Ok(v) => part_vals.push(v),
            Err(err) => {
                heap.drop_values(part_vals)?;
                return Err(err);
            }
        }
    }
    let parts_list = heap.construct("$list", part_vals)?;
    let mut query_vals = Vec::new();
    if let Some(q) = query_str {
        for part in q.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = part.split_once('=').unwrap_or((part, ""));
            let kv = match heap.literal(&Lit::String(form_decode_part(k))) {
                Ok(x) => x,
                Err(err) => {
                    heap.drop_value(parts_list)?;
                    heap.drop_values(query_vals)?;
                    return Err(err);
                }
            };
            let vv = match heap.literal(&Lit::String(form_decode_part(v))) {
                Ok(x) => x,
                Err(err) => {
                    heap.drop_value(kv)?;
                    heap.drop_value(parts_list)?;
                    heap.drop_values(query_vals)?;
                    return Err(err);
                }
            };
            match heap.construct("$tuple", vec![kv, vv]) {
                Ok(t) => query_vals.push(t),
                Err(err) => {
                    heap.drop_value(parts_list)?;
                    heap.drop_values(query_vals)?;
                    return Err(err);
                }
            }
        }
    }
    let query_list = match heap.construct("$list", query_vals) {
        Ok(l) => l,
        Err(err) => {
            heap.drop_value(parts_list)?;
            return Err(err);
        }
    };
    let pair = heap.construct("$tuple", vec![parts_list, query_list])?;
    heap.construct("Prelude.Option.Some", vec![pair])
}

pub(super) fn route_coerce_candidates(
    heap: &mut Heap,
    text_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    let mut items = vec![heap.dup(text_val)?];
    if !js_trim(&text).is_empty() {
        let n = js_number_parse(&text);
        if !n.is_nan() || text == "NaN" {
            items.push(Value::Float(n));
        }
    }
    if text == "true" {
        items.push(Value::Bool(true));
    } else if text == "false" {
        items.push(Value::Bool(false));
    }
    heap.construct("$list", items)
}
