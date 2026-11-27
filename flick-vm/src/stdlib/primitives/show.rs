pub fn show_value(heap: &Heap, value: &Value) -> Result<String, Error> {
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
                ..
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                if tag.starts_with('$') {
                    return Err(unsupported("primitive show of collection"));
                }
                Ok(tag.rsplit('.').next().unwrap_or(tag).to_owned())
            }
            Data::BoxedAggregate { value, .. } => show_value(heap, value),
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
            | Data::ForeignCycle { .. }
            | Data::CliSpec(_)
            | Data::CliMulti(_) => Err(unsupported(
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
        Value::Aggregate {
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
            let cp =
                0x10000 + ((u32::from(u) - 0xD800) << 10) + (u32::from(units[i + 1]) - 0xDC00);
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

/// Formats a drawn property-test case value matching the JS and Go test
/// runners' `preview(value)` output (`JSON.stringify` with `BigInt` as
/// `"<digits>n"`, functions as `"<function>"`, and `Option`/`Result` objects).
pub fn preview_property_value(heap: &Heap, value: &Value) -> Result<String, Error> {
    Ok(preview_property_json(heap, value)?.unwrap_or_else(|| "undefined".to_owned()))
}

fn preview_property_json(heap: &Heap, value: &Value) -> Result<Option<String>, Error> {
    match value {
        Value::Unit => Ok(None),
        Value::Bool(v) => Ok(Some(v.to_string())),
        Value::Int(v) => Ok(Some(v.to_string())),
        Value::Int8(v) => Ok(Some(v.to_string())),
        Value::Int16(v) => Ok(Some(v.to_string())),
        Value::Int64(v) => Ok(Some(v.to_string())),
        Value::UInt8(v) => Ok(Some(v.to_string())),
        Value::UInt16(v) => Ok(Some(v.to_string())),
        Value::UInt32(v) => Ok(Some(v.to_string())),
        Value::UInt64(v) => Ok(Some(v.to_string())),
        Value::Float(v) => Ok(Some(preview_float(*v))),
        Value::Float32(v) => Ok(Some(preview_float(f64::from(*v)))),
        Value::Aggregate {
            tag,
            ordinary_fields,
            fields,
        } => preview_tagged(heap, tag, &fields[..*ordinary_fields]),
        Value::Object(token) => match &heap.object(token.id)?.data {
            Data::String(text) => Ok(Some(json_string(text))),
            Data::Utf16(units) => Ok(Some(show_utf16_string(units))),
            Data::JsonNull => Ok(Some("null".to_owned())),
            Data::BigInt(text) => Ok(Some(json_string(&format!("{}n", bigint_text(text)?)))),
            Data::Closure { .. }
            | Data::Task { .. }
            | Data::Continuation { .. }
            | Data::MaskRestore { .. } => Ok(Some("\"<function>\"".to_owned())),
            Data::BoxedAggregate { value, .. } => preview_property_json(heap, value),
            Data::ListSlice {
                backing,
                offset,
                len,
            } => {
                let Value::Object(btok) = backing else {
                    return Ok(Some("\"<value>\"".to_owned()));
                };
                let Data::Tagged {
                    ordinary_fields,
                    fields,
                    ..
                } = &heap.object(btok.id)?.data
                else {
                    return Ok(Some("\"<value>\"".to_owned()));
                };
                let end = offset.saturating_add(*len).min(*ordinary_fields);
                let start = (*offset).min(end);
                preview_array(heap, &fields[start..end])
            }
            Data::Record(fields) => preview_record(heap, fields),
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
                ..
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                preview_tagged(heap, tag, &fields[..*ordinary_fields])
            }
            _ => Ok(Some("\"<value>\"".to_owned())),
        },
        _ => Ok(Some("\"<value>\"".to_owned())),
    }
}

fn preview_float(v: f64) -> String {
    if !v.is_finite() {
        "null".to_owned()
    } else if v == 0.0 {
        "0".to_owned()
    } else {
        js_number(v)
    }
}

fn preview_array(heap: &Heap, elems: &[Value]) -> Result<Option<String>, Error> {
    let mut out = String::from("[");
    for (idx, elem) in elems.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        match preview_property_json(heap, elem)? {
            Some(s) => out.push_str(&s),
            None => out.push_str("null"),
        }
    }
    out.push(']');
    Ok(Some(out))
}

fn preview_record(heap: &Heap, fields: &[(String, Value)]) -> Result<Option<String>, Error> {
    let mut out = String::from("{");
    let mut first = true;
    for (key, val) in fields {
        let Some(rendered) = preview_property_json(heap, val)? else {
            continue;
        };
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&json_string(key));
        out.push(':');
        out.push_str(&rendered);
    }
    out.push('}');
    Ok(Some(out))
}

fn preview_tagged(heap: &Heap, tag: &str, fields: &[Value]) -> Result<Option<String>, Error> {
    if tag == "$list" || tag == "$tuple" {
        return preview_array(heap, fields);
    }
    let short = tag.rsplit('.').next().unwrap_or(tag);
    match (short, fields) {
        ("None", []) => Ok(Some(r#"{"_id":"Option","_tag":"None"}"#.to_owned())),
        ("Some", [v]) => {
            let mut out = String::from(r#"{"_id":"Option","_tag":"Some""#);
            if let Some(rendered) = preview_property_json(heap, v)? {
                out.push_str(r#","value":"#);
                out.push_str(&rendered);
            }
            out.push('}');
            Ok(Some(out))
        }
        ("Success", [v]) => {
            let mut out = String::from(r#"{"_id":"Result","_tag":"Success""#);
            if let Some(rendered) = preview_property_json(heap, v)? {
                out.push_str(r#","value":"#);
                out.push_str(&rendered);
            }
            out.push('}');
            Ok(Some(out))
        }
        ("Failure", [v]) => {
            let mut out = String::from(r#"{"_id":"Result","_tag":"Failure""#);
            if let Some(rendered) = preview_property_json(heap, v)? {
                out.push_str(r#","failure":"#);
                out.push_str(&rendered);
            }
            out.push('}');
            Ok(Some(out))
        }
        ("Redacted", [_]) => Ok(Some("\"<redacted>\"".to_owned())),
        _ => {
            let mut out = format!(r#"{{"_tag":{}"#, json_string(short));
            for (idx, field) in fields.iter().enumerate() {
                if let Some(rendered) = preview_property_json(heap, field)? {
                    use std::fmt::Write as _;
                    let _ = write!(out, r#","_{idx}":{rendered}"#);
                }
            }
            out.push('}');
            Ok(Some(out))
        }
    }
}
