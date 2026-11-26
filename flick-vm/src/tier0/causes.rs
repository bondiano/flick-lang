//! Immutable owned causes. Representation builders do not activate VM producers.
use super::{Data, Error, Heap, ObjectToken, Value, charge};
use crate::stdlib::{Fuel, FuelCounter, charge as charge_hooks};

/// An owned cause root. Diagnostic identities never construct ownership.
///
/// ```compile_fail,E0277
/// use flick_vm::tier0::CauseToken;
/// fn forge_duplicate(token: &CauseToken) -> CauseToken { Clone::clone(token) }
/// ```
///
/// ```compile_fail,E0451
/// use flick_vm::tier0::CauseToken;
/// let _ = CauseToken { token: unreachable!() };
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct CauseToken {
    token: ObjectToken,
}

impl CauseToken {
    pub(crate) fn into_value(self) -> Value {
        Value::Object(self.token)
    }

    pub(crate) fn object_id(&self) -> super::ObjectId {
        self.token.id
    }

    pub(crate) fn from_cause_value(heap: &Heap, value: Value) -> Result<Self, Error> {
        let Value::Object(token) = value else {
            return Err(Error::ExpectedCause);
        };
        if !matches!(heap.object(token.id)?.data, Data::Cause(_)) {
            return Err(Error::ExpectedCause);
        }
        Ok(Self { token })
    }
}

/// Borrowed diagnostics/input: copying a reason never retains its payload.
#[derive(Clone, Copy, Debug)]
pub enum Reason<'a> {
    Fail(&'a Value),
    Die(&'a Value),
    Interrupt { fiber_id: Option<usize> },
}

#[derive(Debug)]
pub(crate) struct CauseData {
    pub(crate) reasons: Vec<StoredReason>,
    pub(crate) trace: Option<Value>,
}

impl CauseData {
    pub(crate) fn plain(reasons: Vec<StoredReason>) -> Self {
        Self {
            reasons,
            trace: None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum StoredReason {
    Fail(Value),
    Die(Value),
    Interrupt { fiber_id: Option<usize> },
}

impl StoredReason {
    pub(crate) fn borrowed(&self) -> Reason<'_> {
        match self {
            Self::Fail(value) => Reason::Fail(value),
            Self::Die(value) => Reason::Die(value),
            Self::Interrupt { fiber_id } => Reason::Interrupt {
                fiber_id: *fiber_id,
            },
        }
    }

    pub(crate) fn into_value(self) -> Option<Value> {
        match self {
            Self::Fail(value) | Self::Die(value) => Some(value),
            Self::Interrupt { .. } => None,
        }
    }
}

/// A language result. Empty Failure is distinct from Success(Unit).
#[derive(Debug)]
pub enum LanguageExit {
    Success(Value),
    Failure(CauseToken),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefectClass {
    Error,
    RangeError,
    SyntaxError,
    TypeError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefectCode {
    NoMatch,
    MissingHandler { effect: String, op: String },
    IntDivisionByZero { remainder: bool },
    BigIntNegativeExponent,
    BigIntNonIntegralNumber,
    BigIntInvalidString,
    BigIntUndefined,
    ArenaOwnerClosed,
    ArenaClosed,
    ResumeTwice,
    LazyCycle,
    Todo(String),
    Assert(String),
    RangeError(String),
}

/// Structured immutable metadata; each thrown instance gets a fresh heap identity.
#[derive(Debug, PartialEq, Eq)]
pub struct DefectSpec {
    code: DefectCode,
}

impl DefectSpec {
    pub(super) fn arena_owner_closed() -> Self {
        Self {
            code: DefectCode::ArenaOwnerClosed,
        }
    }

    pub(super) fn arena_closed() -> Self {
        Self {
            code: DefectCode::ArenaClosed,
        }
    }

    pub(crate) fn custom(msg: impl Into<String>) -> Self {
        Self {
            code: DefectCode::Assert(msg.into()),
        }
    }

    pub fn code(&self) -> &DefectCode {
        &self.code
    }

    pub(crate) fn message(&self) -> String {
        match &self.code {
            DefectCode::NoMatch => "no matching pattern".into(),
            DefectCode::MissingHandler { effect, op } => {
                format!("unhandled effect operation {effect}.{op}")
            }
            DefectCode::IntDivisionByZero { remainder: false } => {
                "Int.unsafe_div: division by zero".into()
            }
            DefectCode::IntDivisionByZero { remainder: true } => {
                "Int.unsafe_rem: division by zero".into()
            }
            DefectCode::BigIntNegativeExponent => "Exponent must be positive".into(),
            DefectCode::BigIntNonIntegralNumber => {
                "The number cannot be converted to a BigInt because it is not an integer".into()
            }
            DefectCode::BigIntInvalidString => "Cannot convert string to a BigInt".into(),
            DefectCode::BigIntUndefined => "Cannot convert undefined to a BigInt".into(),
            DefectCode::ArenaOwnerClosed => "arena owner closed".into(),
            DefectCode::ArenaClosed => "arena closed".into(),
            DefectCode::ResumeTwice => "continuation resumed more than once".into(),
            DefectCode::LazyCycle => "cyclic lazy initialization".into(),
            DefectCode::Todo(msg) | DefectCode::Assert(msg) | DefectCode::RangeError(msg) => {
                msg.clone()
            }
        }
    }

    pub fn class(&self) -> DefectClass {
        match self.code {
            DefectCode::NoMatch
            | DefectCode::MissingHandler { .. }
            | DefectCode::ArenaOwnerClosed
            | DefectCode::ArenaClosed
            | DefectCode::ResumeTwice
            | DefectCode::LazyCycle
            | DefectCode::Todo(_)
            | DefectCode::Assert(_) => DefectClass::Error,
            DefectCode::IntDivisionByZero { .. }
            | DefectCode::BigIntNegativeExponent
            | DefectCode::BigIntNonIntegralNumber
            | DefectCode::RangeError(_) => DefectClass::RangeError,
            DefectCode::BigIntInvalidString => DefectClass::SyntaxError,
            DefectCode::BigIntUndefined => DefectClass::TypeError,
        }
    }

    fn legacy_error(&self) -> Error {
        match &self.code {
            DefectCode::NoMatch => Error::NoMatch,
            DefectCode::MissingHandler { effect, op } => Error::MissingHandler {
                effect: effect.clone(),
                op: op.clone(),
            },
            DefectCode::IntDivisionByZero { remainder } => Error::IntDivisionByZero {
                remainder: *remainder,
            },
            DefectCode::BigIntNegativeExponent => Error::BigIntNegativeExponent,
            DefectCode::BigIntNonIntegralNumber => Error::BigIntNonIntegralNumber,
            DefectCode::BigIntInvalidString => Error::BigIntInvalidString,
            DefectCode::BigIntUndefined => Error::BigIntUndefined,
            DefectCode::ArenaOwnerClosed => Error::ClosedOwner,
            DefectCode::ArenaClosed => Error::ClosedArena,
            DefectCode::ResumeTwice => Error::ResumeTwice,
            DefectCode::LazyCycle => Error::LazyCycle,
            DefectCode::Todo(msg) => Error::Todo(msg.clone()),
            DefectCode::Assert(msg) => Error::Assert(msg.clone()),
            DefectCode::RangeError(msg) => Error::RangeError(msg.clone()),
        }
    }
}

/// Classification is used at the running operation boundary, never on final Err.
pub(super) enum StepFailure {
    Language(DefectSpec),
    Evaluator(Error),
}

impl StepFailure {
    pub(super) fn classify(error: Error) -> Self {
        let code = match error {
            Error::NoMatch => DefectCode::NoMatch,
            Error::MissingHandler { effect, op } => DefectCode::MissingHandler { effect, op },
            Error::IntDivisionByZero { remainder } => DefectCode::IntDivisionByZero { remainder },
            Error::BigIntNegativeExponent => DefectCode::BigIntNegativeExponent,
            Error::BigIntNonIntegralNumber => DefectCode::BigIntNonIntegralNumber,
            Error::BigIntInvalidString => DefectCode::BigIntInvalidString,
            Error::BigIntUndefined => DefectCode::BigIntUndefined,
            Error::ResumeTwice => DefectCode::ResumeTwice,
            Error::LazyCycle => DefectCode::LazyCycle,
            Error::Todo(msg) => DefectCode::Todo(msg),
            Error::Assert(msg) => DefectCode::Assert(msg),
            Error::RangeError(msg) => DefectCode::RangeError(msg),
            evaluator => return Self::Evaluator(evaluator),
        };
        Self::Language(DefectSpec { code })
    }
}

impl Heap {
    /// Duplicates borrowed payloads in order. On partial failure every acquired
    /// duplicate is released; the caller's original tokens remain untouched.
    pub fn cause(&mut self, reasons: &[Reason<'_>], fuel: &mut u64) -> Result<CauseToken, Error> {
        self.cause_metered(reasons, &mut FuelCounter(fuel))
    }

    pub(crate) fn cause_metered(
        &mut self,
        reasons: &[Reason<'_>],
        fuel: Fuel<'_>,
    ) -> Result<CauseToken, Error> {
        charge_hooks(fuel, reasons.len())?;
        let mut stored = Vec::with_capacity(reasons.len());
        if let Err(error) = self.duplicate_reasons(reasons, fuel, &mut stored) {
            self.drop_values(
                stored
                    .into_iter()
                    .filter_map(StoredReason::into_value)
                    .collect(),
            )?;
            return Err(error);
        }
        let Value::Object(token) = self.allocate(Data::Cause(CauseData::plain(stored))) else {
            unreachable!("cause allocation")
        };
        Ok(CauseToken { token })
    }

    fn duplicate_reasons(
        &mut self,
        reasons: &[Reason<'_>],
        fuel: Fuel<'_>,
        stored: &mut Vec<StoredReason>,
    ) -> Result<(), Error> {
        for reason in reasons {
            charge_hooks(fuel, 1)?;
            let reason = match reason {
                Reason::Fail(value) => {
                    self.admit_ordinary(value)?;
                    StoredReason::Fail(self.dup(value)?)
                }
                Reason::Die(value) => {
                    self.admit_ordinary(value)?;
                    StoredReason::Die(self.dup(value)?)
                }
                Reason::Interrupt { fiber_id } => StoredReason::Interrupt {
                    fiber_id: *fiber_id,
                },
            };
            stored.push(reason);
        }
        charge_hooks(fuel, 1)
    }

    pub(crate) fn cause_data(&self, cause: &CauseToken) -> Result<&[StoredReason], Error> {
        let Data::Cause(data) = &self.object(cause.token.id)?.data else {
            return Err(Error::ExpectedCause);
        };
        Ok(&data.reasons)
    }

    pub fn cause_reasons(
        &self,
        cause: &CauseToken,
    ) -> Result<impl ExactSizeIterator<Item = Reason<'_>>, Error> {
        Ok(self.cause_data(cause)?.iter().map(StoredReason::borrowed))
    }

    pub fn dup_cause(&mut self, cause: &CauseToken) -> Result<CauseToken, Error> {
        self.dup_cause_id(cause.token.id)
    }

    pub(crate) fn dup_cause_id(&mut self, id: super::ObjectId) -> Result<CauseToken, Error> {
        if !matches!(self.object(id)?.data, Data::Cause(_)) {
            return Err(Error::ExpectedCause);
        }
        let Value::Object(token) = self.dup(&Value::Object(ObjectToken { id }))? else {
            unreachable!("cause reference")
        };
        Ok(CauseToken { token })
    }

    pub fn drop_cause(&mut self, cause: CauseToken) -> Result<(), Error> {
        self.drop_value(Value::Object(cause.token))
    }

    pub fn defect(&self, value: &Value) -> Result<&DefectSpec, Error> {
        let Value::Object(token) = value else {
            return Err(Error::ExpectedDefect);
        };
        let Data::Defect(spec) = &self.object(token.id)?.data else {
            return Err(Error::ExpectedDefect);
        };
        Ok(spec)
    }

    pub(super) fn ordinary_failure(&self, cause: &CauseToken) -> Result<Option<&Value>, Error> {
        Ok(match self.cause_data(cause)? {
            [StoredReason::Fail(value)] => Some(value),
            _ => None,
        })
    }

    pub(super) fn first_fail(
        &self,
        cause: &CauseToken,
        fuel: &mut u64,
    ) -> Result<Option<Value>, Error> {
        for reason in self.cause_data(cause)? {
            charge(fuel, 1)?;
            if let StoredReason::Fail(value) = reason {
                return Ok(Some(value.representation()));
            }
        }
        Ok(None)
    }

    pub(super) fn elect_failure(
        &mut self,
        cause: &CauseToken,
        fuel: &mut u64,
    ) -> Result<Option<CauseToken>, Error> {
        let reasons = self.cause_data(cause)?;
        charge(fuel, reasons.len().saturating_mul(2))?;
        let has_die = reasons
            .iter()
            .any(|reason| matches!(reason, StoredReason::Die(_)));
        let has_fail = reasons
            .iter()
            .any(|reason| matches!(reason, StoredReason::Fail(_)));
        if !has_die || !has_fail {
            return Ok(None);
        }
        charge(fuel, reasons.len().saturating_mul(2))?;
        // These are inert representations, NOT newly owned edges. C remains rooted by Machine.
        let copies: Vec<_> = reasons
            .iter()
            .map(|reason| match reason {
                StoredReason::Fail(value) | StoredReason::Die(value) => {
                    StoredReason::Die(value.representation())
                }
                StoredReason::Interrupt { fiber_id } => StoredReason::Interrupt {
                    fiber_id: *fiber_id,
                },
            })
            .collect();
        let borrowed: Vec<_> = copies.iter().map(StoredReason::borrowed).collect();
        self.cause(&borrowed, fuel).map(Some)
    }

    pub fn cause_error(&self, cause: &CauseToken) -> Result<Error, Error> {
        Ok(match self.cause_data(cause)? {
            [StoredReason::Die(value)] => self.defect(value)?.legacy_error(),
            _ => Error::UnhandledFailure,
        })
    }
}

#[cfg(test)]
#[path = "causes_tests.rs"]
mod tests;
