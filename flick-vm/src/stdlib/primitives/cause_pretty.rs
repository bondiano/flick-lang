//! Exact `Cause.pretty` and `Cause.pretty_errors` formatting (matching `runtime-go/flickrt/cause_pretty.go`).

use std::fmt::Write as _;

use flick_core::ir::Lit;
use flick_core::opt::{js_number, json_string};

use super::outcome_ops::{describe_defect, expect_cause_reasons};
use super::{Data, Error, Fuel, Heap, Value, charge};
use crate::tier0::causes::StoredReason;

const INTERRUPTED_MESSAGE: &str = "All fibers interrupted without error";

struct PrettyError {
    name: String,
    message: String,
    stack: String,
    cause: Option<Box<PrettyError>>,
}

impl PrettyError {
    fn rendered(&self) -> String {
        let Some(inner) = &self.cause else {
            return self.stack.clone();
        };
        let lines = inner.stack.replace('\n', "\n  ");
        format!("{} {{\n  [cause]: {lines}\n}}", self.stack)
    }
}

pub(super) fn wire_tag(id: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in id.as_bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    let name = id.rsplit('.').next().unwrap_or(id);
    format!("{name}#{:012x}", h >> 16)
}

fn format_json(heap: &Heap, val: &Value) -> Result<String, Error> {
    match val {
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(js_number(*f)),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::String(s) => Ok(json_string(s)),
            Data::Utf16(u) => Ok(json_string(&String::from_utf16_lossy(u))),
            Data::JsonNull => Ok("null".into()),
            Data::BigInt(s) => Ok(format!("{s}n")),
            Data::ListSlice { .. } => {
                let (_, _, len) = heap.list_view(val)?;
                let mut parts = Vec::with_capacity(len);
                for i in 0..len {
                    let elem = heap.field(val, i)?;
                    parts.push(format_json(heap, elem)?);
                }
                Ok(format!("[{}]", parts.join(",")))
            }
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
                ..
            } => {
                if tag == "$list" || tag == "$tuple" {
                    let mut parts = Vec::with_capacity(*ordinary_fields);
                    for f in &fields[..*ordinary_fields] {
                        parts.push(format_json(heap, f)?);
                    }
                    return Ok(format!("[{}]", parts.join(",")));
                }
                if tag == "Prelude.Option.None" {
                    return Ok(r#"{"_id":"Option","_tag":"None"}"#.into());
                }
                if tag == "Prelude.Option.Some" && *ordinary_fields == 1 {
                    return Ok(format!(
                        r#"{{"_id":"Option","_tag":"Some","value":{}}}"#,
                        format_json(heap, &fields[0])?
                    ));
                }
                if tag == "Prelude.Result.Ok" && *ordinary_fields == 1 {
                    return Ok(format!(
                        r#"{{"_id":"Result","_tag":"Success","value":{}}}"#,
                        format_json(heap, &fields[0])?
                    ));
                }
                if tag == "Prelude.Result.Err" && *ordinary_fields == 1 {
                    return Ok(format!(
                        r#"{{"_id":"Result","_tag":"Failure","failure":{}}}"#,
                        format_json(heap, &fields[0])?
                    ));
                }
                let mut out = format!(r#"{{"_tag":{}"#, json_string(&wire_tag(tag)));
                for (i, f) in fields[..*ordinary_fields].iter().enumerate() {
                    out.push(',');
                    out.push_str(&json_string(&format!("_{i}")));
                    out.push(':');
                    out.push_str(&format_json(heap, f)?);
                }
                out.push('}');
                Ok(out)
            }
            Data::Record(entries) => {
                let mut parts = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    parts.push(format!("{}:{}", json_string(k), format_json(heap, v)?));
                }
                Ok(format!("{{{}}}", parts.join(",")))
            }
            _ => super::show_value(heap, val),
        },
        _ => Ok("null".into()),
    }
}

fn with_trace(heap: &Heap, mut stack: String, trace: Option<&Value>) -> Result<String, Error> {
    let mut cur = trace.map(Value::representation);
    for _ in 0..10 {
        let Some(ref frame_val) = cur else {
            break;
        };
        let Value::Object(tok) = frame_val else {
            break;
        };
        let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
            break;
        };
        if fields.len() < 3 {
            break;
        }
        let name = heap.string_lossy(&fields[0])?;
        let loc = heap.string_lossy(&fields[1])?;
        if loc.is_empty() {
            let _ = write!(stack, "\n    at {name}");
        } else {
            let trimmed = loc.strip_prefix("at ").unwrap_or(&loc);
            let _ = write!(stack, "\n    at {name} ({trimmed})");
        }
        cur = match &fields[2] {
            Value::Object(ptok) => match &heap.object(ptok.id)?.data {
                Data::Tagged {
                    tag, fields: pf, ..
                } if tag == "Prelude.Option.Some" && !pf.is_empty() => Some(pf[0].representation()),
                _ => None,
            },
            _ => None,
        };
    }
    Ok(stack)
}

fn pretty_failure(heap: &Heap, err: &Value, trace: Option<&Value>) -> Result<PrettyError, Error> {
    let mut name = "Error".to_owned();
    let mut message = format_json(heap, err)?;
    if let Value::Object(tok) = err
        && let Data::Record(entries) = &heap.object(tok.id)?.data
    {
        for (k, v) in entries {
            if k == "message"
                && let Ok(s) = heap.string(v)
            {
                s.clone_into(&mut message);
            } else if k == "name"
                && let Ok(s) = heap.string(v)
            {
                s.clone_into(&mut name);
            }
        }
    }
    let stack = with_trace(heap, format!("{name}: {message}"), trace)?;
    Ok(PrettyError {
        name,
        message,
        stack,
        cause: None,
    })
}

fn pretty_defect(heap: &Heap, d: &Value, trace: Option<&Value>) -> Result<PrettyError, Error> {
    if let Value::Object(tok) = d
        && matches!(&heap.object(tok.id)?.data, Data::Tagged { tag, .. } if !tag.starts_with('$'))
    {
        return pretty_failure(heap, d, trace);
    }
    let raw = describe_defect(heap, d)?;
    let text = raw.strip_suffix('\n').unwrap_or(&raw);
    let first = text.split('\n').next().unwrap_or("");
    if let Some((name, msg)) = first.split_once(": ")
        && matches!(
            name,
            "Error"
                | "TypeError"
                | "RangeError"
                | "SyntaxError"
                | "ReferenceError"
                | "URIError"
                | "EvalError"
        )
    {
        let stack = with_trace(heap, text.to_owned(), trace)?;
        return Ok(PrettyError {
            name: name.to_owned(),
            message: msg.to_owned(),
            stack,
            cause: None,
        });
    }
    let body = if text.is_empty() {
        "Unknown error: "
    } else {
        text
    };
    let stack = with_trace(heap, format!("Error: {body}"), trace)?;
    Ok(PrettyError {
        name: "Error".to_owned(),
        message: body.to_owned(),
        stack,
        cause: None,
    })
}

fn collect_pretty_errors(heap: &Heap, cause: &Value) -> Result<Vec<PrettyError>, Error> {
    let Value::Object(tok) = cause else {
        return Err(Error::ExpectedCause);
    };
    let Data::Cause(data) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedCause);
    };
    let trace = data.trace.as_ref();
    let mut out = Vec::new();
    let mut interrupts: Vec<Option<usize>> = Vec::new();
    for r in &data.reasons {
        match r {
            StoredReason::Interrupt { fiber_id } => interrupts.push(*fiber_id),
            StoredReason::Fail(e) => out.push(pretty_failure(heap, e, trace)?),
            StoredReason::Die(d) => out.push(pretty_defect(heap, d, trace)?),
        }
    }
    if !out.is_empty() || interrupts.is_empty() {
        return Ok(out);
    }
    let mut lines = vec!["InterruptCause: The fiber was interrupted by:".to_owned()];
    for fid in interrupts {
        match fid {
            Some(id) if id != 0 => lines.push(format!("    at fiber (#{id})")),
            _ => lines.push("    at fiber (unknown)".to_owned()),
        }
    }
    let inner = PrettyError {
        name: "InterruptCause".into(),
        message: "The fiber was interrupted by:".into(),
        stack: lines.join("\n"),
        cause: None,
    };
    let stack = with_trace(
        heap,
        format!("InterruptError: {INTERRUPTED_MESSAGE}"),
        trace,
    )?;
    Ok(vec![PrettyError {
        name: "InterruptError".into(),
        message: INTERRUPTED_MESSAGE.into(),
        stack,
        cause: Some(Box::new(inner)),
    }])
}

pub(super) fn cause_pretty(heap: &mut Heap, cause: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let rs = expect_cause_reasons(heap, cause)?;
    charge(fuel, rs.len())?;
    let errs = collect_pretty_errors(heap, cause)?;
    let rendered: Vec<String> = errs.iter().map(PrettyError::rendered).collect();
    heap.literal(&Lit::String(rendered.join("\n")))
}

pub(super) fn cause_pretty_errors(
    heap: &mut Heap,
    cause: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let rs = expect_cause_reasons(heap, cause)?;
    charge(fuel, rs.len())?;
    let errs = collect_pretty_errors(heap, cause)?;
    let mut vals = Vec::with_capacity(errs.len());
    for e in errs {
        let text = format!("{}: {}", e.name, e.message);
        match heap.literal(&Lit::String(text)) {
            Ok(v) => vals.push(v),
            Err(err) => {
                heap.drop_values(vals)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", vals)
}
