//! Formatting of uncaught Flick values and causes for host stderr, matching
//! JS `entry.ts` and Go `platform.go`.

use crate::stdlib::primitives::describe_defect;
use crate::stdlib::show_value;
use crate::tier0::{CauseToken, Data, Error, Heap, Reason, Value};

pub(super) fn record_field<'a>(
    heap: &'a Heap,
    rec: &Value,
    name: &str,
) -> Result<&'a Value, Error> {
    let Value::Object(tok) = rec else {
        return Err(Error::ExpectedRecord);
    };
    let Data::Record(fields) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedRecord);
    };
    fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .ok_or_else(|| Error::MissingNamedField(name.to_owned()))
}

/// Extracts `message` from a `Prelude.AppError.AppError { message, details }` value.
pub(crate) fn app_error_message(heap: &Heap, val: &Value) -> Option<String> {
    if heap.tag(val).ok()? != "Prelude.AppError.AppError" {
        return None;
    }
    let rec = heap.field(val, 0).ok()?;
    let msg = record_field(heap, rec, "message").ok()?;
    heap.string_lossy(msg).ok()
}

/// Formats an uncaught raised value for CLI stderr, matching JS `entry.ts` and Go `platform.go`.
pub fn format_uncaught_raised(heap: &Heap, val: &Value) -> String {
    if let Some(msg) = app_error_message(heap, val) {
        return format!("error: {msg}");
    }
    show_value(heap, val).unwrap_or_else(|_| "error: unhandled failure".to_owned())
}

/// Formats an uncaught `CauseToken` for CLI stderr, matching JS `entry.ts` and Go `platform.go`.
pub fn format_uncaught_cause(heap: &Heap, cause: &CauseToken) -> String {
    let Ok(reasons) = heap.cause_reasons(cause) else {
        return "error: unhandled failure".to_owned();
    };
    let reasons: Vec<_> = reasons.collect();
    let has_die = reasons.iter().any(|r| matches!(r, Reason::Die(_)));
    if !has_die {
        for r in &reasons {
            if let Reason::Fail(v) = r
                && let Some(msg) = app_error_message(heap, v)
            {
                return format!("error: {msg}");
            }
        }
    }
    let mut parts = Vec::new();
    for r in reasons {
        match r {
            Reason::Fail(v) => {
                if let Some(msg) = app_error_message(heap, v) {
                    parts.push(msg);
                } else if let Ok(s) = show_value(heap, v) {
                    parts.push(s);
                }
            }
            Reason::Die(d) => {
                if let Ok(s) = describe_defect(heap, d) {
                    parts.push(s);
                }
            }
            Reason::Interrupt { .. } => {}
        }
    }
    if parts.is_empty() {
        "error: main was interrupted".to_owned()
    } else {
        parts.join("\n")
    }
}
