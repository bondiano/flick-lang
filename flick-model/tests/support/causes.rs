//! Outcome migration helpers: semantic expectations still require exact Die codes.
use flick_model::memory::{
    CauseToken, DefectClass, DefectCode, Error, Heap, ProgramOutcome, Reason,
};

fn inspect_defect(heap: &Heap, cause: &CauseToken) -> Result<Error, String> {
    let mut reasons = heap
        .cause_reasons(cause)
        .map_err(|error| error.to_string())?;
    let Some(Reason::Die(value)) = reasons.next() else {
        return Err("expected a singleton Die".into());
    };
    if reasons.next().is_some() {
        return Err("never accept arbitrary compound causes".into());
    }
    let defect = heap.defect(value).map_err(|error| error.to_string())?;
    let (error, class) = match defect.code() {
        DefectCode::NoMatch => (Error::NoMatch, DefectClass::Error),
        DefectCode::MissingHandler { effect, op } => (
            Error::MissingHandler {
                effect: effect.clone(),
                op: op.clone(),
            },
            DefectClass::Error,
        ),
        DefectCode::IntDivisionByZero { remainder } => (
            Error::IntDivisionByZero {
                remainder: *remainder,
            },
            DefectClass::RangeError,
        ),
        DefectCode::BigIntNegativeExponent => {
            (Error::BigIntNegativeExponent, DefectClass::RangeError)
        }
        DefectCode::BigIntNonIntegralNumber => {
            (Error::BigIntNonIntegralNumber, DefectClass::RangeError)
        }
        DefectCode::BigIntInvalidString => (Error::BigIntInvalidString, DefectClass::SyntaxError),
        DefectCode::BigIntUndefined => (Error::BigIntUndefined, DefectClass::TypeError),
        DefectCode::ArenaOwnerClosed => (Error::ClosedOwner, DefectClass::Error),
        DefectCode::ArenaClosed => (Error::ClosedArena, DefectClass::Error),
        DefectCode::ResumeTwice => (Error::ResumeTwice, DefectClass::Error),
        DefectCode::LazyCycle => (Error::LazyCycle, DefectClass::Error),
        DefectCode::Todo(msg) => (Error::Todo(msg.clone()), DefectClass::Error),
        DefectCode::Assert(msg) => (Error::Assert(msg.clone()), DefectClass::Error),
        DefectCode::RangeError(msg) => (Error::RangeError(msg.clone()), DefectClass::RangeError),
    };
    if defect.class() != class {
        return Err("wrong structured defect class".into());
    }
    Ok(error)
}

pub fn defect_error(heap: &mut Heap, cause: CauseToken) -> Error {
    let result = inspect_defect(heap, &cause);
    heap.drop_cause(cause)
        .expect("inspected cause explicitly released");
    result.expect("exact singleton structured Die")
}

pub fn assert_defect(heap: &mut Heap, outcome: ProgramOutcome, expected: &Error) {
    match outcome {
        ProgramOutcome::Failed(cause) => assert_eq!(&defect_error(heap, cause), expected),
        other => unexpected(heap, other, "expected exact semantic Die"),
    }
}

pub fn unexpected(heap: &mut Heap, outcome: ProgramOutcome, context: &str) -> ! {
    let diagnostic = format!("{context}: {outcome:?}");
    outcome
        .release(heap)
        .expect("unexpected owned outcome is explicitly released");
    panic!("{diagnostic}")
}
