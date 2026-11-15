//! Immutable owned causes. Representation builders do not activate VM producers.
use super::{Data, Error, Heap, ObjectToken, Value, charge};

/// An owned cause root. Diagnostic identities never construct ownership.
///
/// ```compile_fail,E0277
/// use flick_model::memory::CauseToken;
/// fn forge_duplicate(token: &CauseToken) -> CauseToken { Clone::clone(token) }
/// ```
///
/// ```compile_fail,E0451
/// use flick_model::memory::CauseToken;
/// let _ = CauseToken { token: unreachable!() };
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct CauseToken {
    token: ObjectToken,
}

impl CauseToken {
    pub(super) fn into_value(self) -> Value {
        Value::Object(self.token)
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
pub(super) enum StoredReason {
    Fail(Value),
    Die(Value),
    Interrupt { fiber_id: Option<usize> },
}

impl StoredReason {
    fn borrowed(&self) -> Reason<'_> {
        match self {
            Self::Fail(value) => Reason::Fail(value),
            Self::Die(value) => Reason::Die(value),
            Self::Interrupt { fiber_id } => Reason::Interrupt {
                fiber_id: *fiber_id,
            },
        }
    }

    pub(super) fn into_value(self) -> Option<Value> {
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

    pub fn code(&self) -> &DefectCode {
        &self.code
    }

    pub fn class(&self) -> DefectClass {
        match self.code {
            DefectCode::NoMatch
            | DefectCode::MissingHandler { .. }
            | DefectCode::ArenaOwnerClosed
            | DefectCode::ArenaClosed => DefectClass::Error,
            DefectCode::IntDivisionByZero { .. }
            | DefectCode::BigIntNegativeExponent
            | DefectCode::BigIntNonIntegralNumber => DefectClass::RangeError,
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
            evaluator @ (Error::InvalidFunction(_)
            | Error::InvalidProgram(_)
            | Error::NeedsProgram
            | Error::UnsupportedOperation(_)
            | Error::OperationArgumentCount { .. }
            | Error::HandlerIdentityOverflow
            | Error::ExpectedResult
            | Error::UnhandledFailure
            | Error::OutOfFuel
            | Error::ConstructorFieldOverflow
            | Error::InvalidTaggedBoundary { .. }
            | Error::ReservedConstructorEvidence(_)
            | Error::PatternEvidenceLayout { .. }
            | Error::PatternKind(_)
            | Error::PatternLayout { .. }
            | Error::UnknownGlobal(_)
            | Error::GlobalCycle(_)
            | Error::ExpectedServiceTag
            | Error::ExpectedLayer
            | Error::ExpectedClosure
            | Error::ForeignClosure
            | Error::ExpectedTask
            | Error::ForeignTask
            | Error::ExpectedCell
            | Error::ForeignCell
            | Error::ClosedCell
            | Error::ExpectedModifyPair
            | Error::ExpectedOwner
            | Error::ForeignOwner
            | Error::ClosedOwner
            | Error::ExpectedArena
            | Error::ExpectedArenaView
            | Error::ForeignArena
            | Error::ClosedArena
            | Error::MissingArenaSlot(_)
            | Error::ExpectedRecord
            | Error::ExpectedDictionary
            | Error::MissingMethod(_)
            | Error::ExpectedStructure
            | Error::StructureArity { .. }
            | Error::MissingNamedField(_)
            | Error::DuplicateField(_)
            | Error::UnsupportedPrimitive(_)
            | Error::InvalidBigInt(_)
            | Error::ArgumentCount { .. }
            | Error::InvalidObject(_)
            | Error::ReferenceOverflow(_)
            | Error::InsufficientReferences { .. }
            | Error::ExpectedObject
            | Error::ExpectedString
            | Error::ExpectedBigInt
            | Error::ExpectedBoolean
            | Error::MissingField { .. }
            | Error::NoToken(_)
            | Error::Leftover(_)
            | Error::LiveObjects(_)
            | Error::LiveCountMismatch { .. }
            | Error::ExpectedCause
            | Error::ExpectedDefect
            | Error::RecoveryOutsideSelection
            | Error::ExpectedRecoveryResult
            | Error::ForeignRecoveryResult
            | Error::RecoveryEscape
            | Error::RecoveryIdentityOverflow) => return Self::Evaluator(evaluator),
        };
        Self::Language(DefectSpec { code })
    }
}

impl Heap {
    /// Duplicates borrowed payloads in order. On partial failure every acquired
    /// duplicate is released; the caller's original tokens remain untouched.
    pub fn cause(&mut self, reasons: &[Reason<'_>], fuel: &mut u64) -> Result<CauseToken, Error> {
        charge(fuel, reasons.len())?;
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
        let Value::Object(token) = self.allocate(Data::Cause(stored)) else {
            unreachable!("cause allocation")
        };
        Ok(CauseToken { token })
    }

    fn duplicate_reasons(
        &mut self,
        reasons: &[Reason<'_>],
        fuel: &mut u64,
        stored: &mut Vec<StoredReason>,
    ) -> Result<(), Error> {
        for reason in reasons {
            charge(fuel, 1)?;
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
        charge(fuel, 1)
    }

    fn cause_data(&self, cause: &CauseToken) -> Result<&[StoredReason], Error> {
        let Data::Cause(reasons) = &self.object(cause.token.id)?.data else {
            return Err(Error::ExpectedCause);
        };
        Ok(reasons)
    }

    pub fn cause_reasons(
        &self,
        cause: &CauseToken,
    ) -> Result<impl ExactSizeIterator<Item = Reason<'_>>, Error> {
        Ok(self.cause_data(cause)?.iter().map(StoredReason::borrowed))
    }

    pub fn dup_cause(&mut self, cause: &CauseToken) -> Result<CauseToken, Error> {
        self.cause_data(cause)?;
        let Value::Object(token) = self.dup(&Value::Object(ObjectToken { id: cause.token.id }))?
        else {
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

    pub(crate) fn cause_error(&self, cause: &CauseToken) -> Result<Error, Error> {
        Ok(match self.cause_data(cause)? {
            [StoredReason::Die(value)] => self.defect(value)?.legacy_error(),
            _ => Error::UnhandledFailure,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flick_core::ir::Lit;

    #[test]
    fn overflow_after_a_valid_duplicate_prefix_releases_every_accepted_edge() {
        let mut heap = Heap::new();
        let first = heap
            .literal(&Lit::String("first".into()))
            .expect("first root");
        let overflow = heap
            .literal(&Lit::String("overflow".into()))
            .expect("overflow root");
        let id = overflow.object_id().expect("object identity");
        heap.object_mut(id)
            .expect("private overflow fixture")
            .references = usize::MAX;
        assert_eq!(
            heap.cause(&[Reason::Fail(&first), Reason::Die(&overflow)], &mut 100),
            Err(Error::ReferenceOverflow(id))
        );
        assert_eq!(heap.reference_count(&first), Ok(1));
        assert_eq!(heap.reference_count(&overflow), Ok(usize::MAX));
        heap.object_mut(id)
            .expect("restore fixture reference count")
            .references = 1;
        heap.drop_values(vec![first, overflow])
            .expect("original roots released");
        heap.finish().expect("accepted duplicates reclaimed");
    }

    #[test]
    fn malformed_payload_after_valid_prefix_does_not_lose_accepted_duplicates() {
        let mut heap = Heap::new();
        let first = heap
            .literal(&Lit::String("first".into()))
            .expect("first root");
        let stale = heap
            .literal(&Lit::String("stale".into()))
            .expect("stale root");
        let id = stale.object_id().expect("diagnostic identity only");
        heap.drop_value(stale).expect("retire original object");
        // Private integrity injection, deliberately impossible through public token construction.
        let invalid = Value::Object(ObjectToken { id });
        assert_eq!(
            heap.cause(&[Reason::Fail(&first), Reason::Die(&invalid)], &mut 100),
            Err(Error::InvalidObject(id))
        );
        assert_eq!(heap.reference_count(&first), Ok(1));
        heap.drop_value(first).expect("original release");
        heap.finish()
            .expect("failed builder drains accepted duplicates");
    }

    #[test]
    fn cause_duplicate_overflow_and_malformed_kind_do_not_mutate_owned_edges() {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("original".into()))
            .expect("payload");
        let cause = heap
            .cause(&[Reason::Die(&payload)], &mut 10)
            .expect("cause");
        let id = cause.token.id;
        heap.object_mut(id)
            .expect("private overflow fixture")
            .references = usize::MAX;
        assert_eq!(heap.dup_cause(&cause), Err(Error::ReferenceOverflow(id)));
        assert_eq!(heap.reference_count(&payload), Ok(2));
        heap.object_mut(id)
            .expect("restore fixture count")
            .references = 1;
        let invalid = CauseToken {
            token: ObjectToken {
                id: payload.object_id().expect("payload identity"),
            },
        };
        assert_eq!(heap.dup_cause(&invalid), Err(Error::ExpectedCause));
        heap.drop_cause(cause).expect("actual owned cause release");
        heap.drop_value(payload).expect("original release");
        let stats = heap.finish().expect("no dropped cause edge was lost");
        assert_eq!(stats.allocations, stats.frees);
    }
    #[test]
    fn closed_resource_errors_are_not_globally_promoted() {
        assert!(matches!(
            StepFailure::classify(Error::ClosedOwner),
            StepFailure::Evaluator(Error::ClosedOwner)
        ));
        assert!(matches!(
            StepFailure::classify(Error::ClosedArena),
            StepFailure::Evaluator(Error::ClosedArena)
        ));
    }
}
