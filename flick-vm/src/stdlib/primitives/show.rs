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
