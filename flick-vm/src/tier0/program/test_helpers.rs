//! Shared test-runner value inspection and assertion formatting helpers used by
//! both Tier 0 and Tier 1 test runners.

use crate::stdlib::primitives;
use crate::tier0::{CauseToken, Data, Error, Heap, LanguageExit, Reason, TestCaseResult, Value};

pub(crate) const fn skipped_case(name: String) -> TestCaseResult {
    TestCaseResult {
        name,
        passed: true,
        skipped: true,
        message: None,
    }
}

pub(crate) fn collect_assertion_outcome(
    heap: &mut Heap,
    name: String,
    exit: LanguageExit,
) -> Result<TestCaseResult, Error> {
    let val = match exit {
        LanguageExit::Success(v) => v,
        LanguageExit::Failure(cause) => {
            let msg = describe_test_failure(heap, &cause)?;
            heap.drop_cause(cause)?;
            return Ok(TestCaseResult {
                name,
                passed: false,
                skipped: false,
                message: Some(msg),
            });
        }
    };
    let tag = heap.tag(&val)?.to_owned();
    let res = if tag == "Test.Assertion.Pass" {
        TestCaseResult {
            name,
            passed: true,
            skipped: false,
            message: None,
        }
    } else if tag == "Test.Assertion.Failed" {
        let msg = string_field(heap, &val, 0)?;
        TestCaseResult {
            name,
            passed: false,
            skipped: false,
            message: Some(msg),
        }
    } else {
        heap.drop_value(val)?;
        return Err(Error::UnsupportedOperation(
            "unexpected test assertion value",
        ));
    };
    heap.drop_value(val)?;
    Ok(res)
}

pub(crate) fn describe_test_failure(heap: &Heap, cause: &CauseToken) -> Result<String, Error> {
    let mut parts = Vec::new();
    for reason in heap.cause_reasons(cause)? {
        match reason {
            Reason::Fail(v) => {
                if let Ok("Test.AssertionFailed.AssertionFailed") = heap.tag(v) {
                    parts.push(string_field(heap, v, 0)?);
                } else {
                    parts.push(format!("fail: {}", primitives::show_value(heap, v)?));
                }
            }
            Reason::Die(d) => {
                parts.push(format!("die: {}", primitives::describe_defect(heap, d)?));
            }
            Reason::Interrupt { .. } => parts.push("interrupted".into()),
        }
    }
    Ok(parts.join("; "))
}

pub(crate) fn string_field(heap: &Heap, val: &Value, index: usize) -> Result<String, Error> {
    let field = heap.field(val, index)?;
    let Value::Object(tok) = field else {
        return Err(Error::UnsupportedOperation("expected string field"));
    };
    match &heap.object(tok.id)?.data {
        Data::String(s) => Ok(s.clone()),
        Data::Utf16(u) => Ok(String::from_utf16_lossy(u)),
        _ => Err(Error::UnsupportedOperation("expected string field")),
    }
}

pub(crate) fn list_reps(heap: &Heap, val: &Value) -> Result<Vec<Value>, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::UnsupportedOperation("expected list"));
    };
    match &heap.object(tok.id)?.data {
        Data::Tagged { tag, fields, .. } if tag == "$list" => {
            Ok(fields.iter().map(Value::representation).collect())
        }
        Data::ListSlice {
            backing,
            offset,
            len,
        } => {
            let Value::Object(btok) = backing else {
                return Err(Error::UnsupportedOperation("invalid list slice backing"));
            };
            let Data::Tagged { fields, .. } = &heap.object(btok.id)?.data else {
                return Err(Error::UnsupportedOperation("invalid list slice backing"));
            };
            Ok(fields[*offset..*offset + *len]
                .iter()
                .map(Value::representation)
                .collect())
        }
        _ => Err(Error::UnsupportedOperation("expected list")),
    }
}
