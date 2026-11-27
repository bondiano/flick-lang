//! Value rendering and cause classification for REPL output.

use std::fmt::Write as _;

use crate::stdlib::primitives;
use crate::tier0::{
    CauseToken, Data, DefectCode, Error, Heap, ObjectId, ObjectToken, Reason, Value,
};

const MAX_DEPTH: usize = 32;

/// Renders a live heap value in readable Flick syntax (temporary until `#1005`).
pub fn render_value(heap: &Heap, value: &Value) -> Result<String, Error> {
    let mut out = String::new();
    render_inner(heap, value, 0, false, &mut out)?;
    Ok(out)
}

fn render_inner(
    heap: &Heap,
    value: &Value,
    depth: usize,
    atom: bool,
    out: &mut String,
) -> Result<(), Error> {
    if depth >= MAX_DEPTH {
        out.push_str("...");
        return Ok(());
    }
    match value {
        Value::Unit => out.push_str("()"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Int8(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Int16(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Int64(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt8(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt16(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt32(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt64(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(f) => render_float(*f, out),
        Value::Float32(f) => render_float(f64::from(*f), out),
        Value::Object(token) => render_object(heap, token.id, depth, atom, out)?,
        Value::Aggregate {
            tag,
            ordinary_fields,
            fields,
        } => render_tagged(
            heap,
            tag,
            &fields[..(*ordinary_fields).min(fields.len())],
            depth,
            atom,
            out,
        )?,
        Value::Cell(_) => out.push_str("<ref>"),
        Value::Var(_) => out.push_str("<var>"),
        Value::Lazy(_) => out.push_str("<lazy>"),
        Value::Iterator(_) => out.push_str("<iterator>"),
        Value::Owner(_) => out.push_str("<owner>"),
        Value::Arena(_) => out.push_str("<arena>"),
        Value::ArenaView(_) => out.push_str("<arena-view>"),
        Value::ReuseNull => out.push_str("<reuse-null>"),
    }
    Ok(())
}

fn render_float(f: f64, out: &mut String) {
    if f.is_nan() {
        out.push_str("NaN");
    } else if f.is_infinite() {
        out.push_str(if f.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        });
    } else if f.fract() == 0.0 && f.abs() < 1e16 {
        let _ = write!(out, "{f:.1}");
    } else {
        let _ = write!(out, "{f}");
    }
}

fn render_object(
    heap: &Heap,
    id: ObjectId,
    depth: usize,
    atom: bool,
    out: &mut String,
) -> Result<(), Error> {
    match &heap.object(id)?.data {
        Data::String(text) => {
            let _ = write!(out, "{text:?}");
        }
        Data::Utf16(units) => {
            let text = String::from_utf16_lossy(units);
            let _ = write!(out, "{text:?}");
        }
        Data::BigInt(text) => {
            let _ = write!(out, "{text}n");
        }
        Data::JsonNull => out.push_str("null"),
        Data::Bytes(bytes) => {
            let _ = write!(out, "<bytes:{}>", bytes.len());
        }
        Data::BoxedAggregate { value, .. } => render_inner(heap, value, depth, atom, out)?,
        Data::Tagged {
            tag,
            ordinary_fields,
            fields,
            ..
        } => {
            let visible = &fields[..(*ordinary_fields).min(fields.len())];
            render_tagged(heap, tag, visible, depth, atom, out)?;
        }
        Data::ListSlice {
            backing,
            offset,
            len,
        } => {
            let Value::Object(btok) = backing else {
                out.push_str("[]");
                return Ok(());
            };
            let Data::Tagged { fields, .. } = &heap.object(btok.id)?.data else {
                out.push_str("[]");
                return Ok(());
            };
            render_sequence(heap, &fields[*offset..*offset + *len], depth, out)?;
        }
        Data::Record(fields) => {
            if fields.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{ ");
                for (i, (label, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str("; ");
                    }
                    let _ = write!(out, "{label} = ");
                    render_inner(heap, val, depth + 1, false, out)?;
                }
                out.push_str(" }");
            }
        }
        Data::Closure { .. } | Data::MaskRestore { .. } => out.push_str("<fn>"),
        Data::Task { .. }
        | Data::MaskedTask { .. }
        | Data::CachedRunTask { .. }
        | Data::CachedInvalidateTask { .. }
        | Data::ExitTask(_) => out.push_str("<task>"),
        Data::Layer(_) => out.push_str("<layer>"),
        Data::Context(_) => out.push_str("<context>"),
        Data::ServiceTag { key } => {
            let short = key.rsplit('.').next().unwrap_or(key);
            let _ = write!(out, "<service {short}>");
        }
        Data::BigDecimal { .. } | Data::RegExp { .. } | Data::Duration(_) => {
            let val = Value::Object(ObjectToken { id });
            out.push_str(&primitives::show_value(heap, &val)?);
        }
        Data::Defect(spec) => {
            let _ = write!(out, "<defect: {}>", spec.message());
        }
        Data::Label(label) => {
            let _ = write!(out, "{label:?}");
        }
        _ => out.push_str("<value>"),
    }
    Ok(())
}

fn render_sequence(
    heap: &Heap,
    items: &[Value],
    depth: usize,
    out: &mut String,
) -> Result<(), Error> {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        render_inner(heap, item, depth + 1, false, out)?;
    }
    out.push(']');
    Ok(())
}

fn render_tagged(
    heap: &Heap,
    tag: &str,
    fields: &[Value],
    depth: usize,
    atom: bool,
    out: &mut String,
) -> Result<(), Error> {
    if tag == "$list" {
        return render_sequence(heap, fields, depth, out);
    }
    if tag == "$tuple" {
        out.push('(');
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            render_inner(heap, field, depth + 1, false, out)?;
        }
        out.push(')');
        return Ok(());
    }
    let short = tag.rsplit('.').next().unwrap_or(tag);
    if fields.is_empty() {
        out.push_str(short);
        return Ok(());
    }
    if atom {
        out.push('(');
    }
    out.push_str(short);
    for field in fields {
        out.push(' ');
        render_inner(heap, field, depth + 1, true, out)?;
    }
    if atom {
        out.push(')');
    }
    Ok(())
}

/// Checks whether a failed cause represents an unhandled effect operation.
pub(crate) fn extract_unhandled_effect(
    heap: &Heap,
    cause: &CauseToken,
) -> Option<(String, String)> {
    let reasons = heap.cause_reasons(cause).ok()?;
    for reason in reasons {
        if let Reason::Die(val) = reason
            && let Ok(spec) = heap.defect(val)
            && let DefectCode::MissingHandler { effect, op } = spec.code()
        {
            return Some((effect.clone(), op.clone()));
        }
    }
    None
}

/// Checks whether a failed cause consists solely of interruption reasons.
pub(crate) fn is_interrupt_cause(heap: &Heap, cause: &CauseToken) -> bool {
    heap.cause_reasons(cause).is_ok_and(|mut reasons| {
        reasons.len() > 0 && reasons.all(|r| matches!(r, Reason::Interrupt { .. }))
    })
}

/// Formats a non-ordinary failure cause for display.
pub(crate) fn describe_cause(heap: &Heap, cause: &CauseToken) -> String {
    let Ok(reasons) = heap.cause_reasons(cause) else {
        return "runtime failure".to_owned();
    };
    let mut parts = Vec::new();
    for reason in reasons {
        match reason {
            Reason::Fail(v) => {
                let text = render_value(heap, v).unwrap_or_else(|_| "<error>".to_owned());
                parts.push(text);
            }
            Reason::Die(d) => {
                let text = primitives::describe_defect(heap, d)
                    .unwrap_or_else(|_| "runtime defect".to_owned());
                parts.push(text);
            }
            Reason::Interrupt { .. } => parts.push("interrupted".to_owned()),
        }
    }
    if parts.is_empty() {
        "runtime failure".to_owned()
    } else {
        parts.join("; ")
    }
}
