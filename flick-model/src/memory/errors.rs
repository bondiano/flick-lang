#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidFunction(memory::Error),
    InvalidProgram(memory::Error),
    NeedsProgram,
    UnsupportedOperation(&'static str),
    MissingHandler {
        effect: String,
        op: String,
    },
    OperationArgumentCount {
        effect: String,
        op: String,
        expected_dicts: usize,
        actual_dicts: usize,
        expected_args: usize,
        actual_args: usize,
    },
    HandlerIdentityOverflow,
    ExpectedResult,
    ExpectedCause,
    ExpectedDefect,
    RecoveryOutsideSelection,
    ExpectedRecoveryResult,
    ForeignRecoveryResult,
    RecoveryEscape,
    RecoveryIdentityOverflow,
    UnhandledFailure,
    OutOfFuel,
    NoMatch,
    ConstructorFieldOverflow,
    InvalidTaggedBoundary {
        ordinary: usize,
        physical: usize,
    },
    ReservedConstructorEvidence(String),
    PatternEvidenceLayout {
        expected_fields: usize,
        actual_fields: usize,
        expected_dicts: usize,
        actual_dicts: usize,
    },
    PatternKind(&'static str),
    PatternLayout {
        expected: usize,
        actual: usize,
    },
    UnknownGlobal(memory::Name),
    GlobalCycle(memory::Name),
    ExpectedServiceTag,
    ExpectedLayer,
    ExpectedClosure,
    ForeignClosure,
    ExpectedTask,
    ForeignTask,
    ExpectedCell,
    ForeignCell,
    ClosedCell,
    ExpectedVar,
    ForeignVar,
    ClosedVar,
    ExpectedLazy,
    ForeignLazy,
    ClosedLazy,
    LazyCycle,
    ResumeTwice,
    ForeignContinuation,
    MissingLoopTarget(String),
    MissingFunctionBoundary,
    MissingGeneratorBoundary,
    ExpectedSeq,
    ForeignSeq,
    ExpectedIterator,
    ForeignIterator,
    ClosedIterator,
    ExpectedLabel,
    ExpectedExit,
    ExpectedOption,
    ExpectedInt,
    ExpectedFloat,
    Todo(String),
    Assert(String),
    RangeError(String),
    ExpectedModifyPair,
    ExpectedOwner,
    ForeignOwner,
    ClosedOwner,
    ExpectedArena,
    ExpectedArenaView,
    ForeignArena,
    ClosedArena,
    MissingArenaSlot(usize),
    ExpectedRecord,
    ExpectedDictionary,
    MissingMethod(String),
    ExpectedStructure,
    StructureArity {
        expected: usize,
        actual: usize,
    },
    MissingNamedField(String),
    DuplicateField(String),
    UnsupportedPrimitive(String),
    /// A source primitive defect, distinct from typed failure and unsupported IR.
    IntDivisionByZero {
        remainder: bool,
    },
    BigIntNegativeExponent,
    BigIntNonIntegralNumber,
    BigIntInvalidString,
    BigIntUndefined,
    InvalidBigInt(String),
    ArgumentCount {
        expected: usize,
        actual: usize,
    },
    InvalidObject(ObjectId),
    ReferenceOverflow(ObjectId),
    InsufficientReferences {
        object: ObjectId,
        required: usize,
        actual: usize,
    },
    ExpectedObject,
    ExpectedString,
    ExpectedBigInt,
    ExpectedBoolean,
    MissingField {
        object: ObjectId,
        field: usize,
    },
    NoToken(Local),
    Leftover(Vec<(Local, usize)>),
    LiveObjects(usize),
    LiveCountMismatch {
        tracked: usize,
        actual: usize,
    },
}

impl Error {
    fn fmt_counts(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientReferences {
                object,
                required,
                actual,
            } => {
                write!(
                    f,
                    "object {object:?} has {actual} references, needs {required}"
                )
            }
            Self::LiveCountMismatch { tracked, actual } => write!(
                f,
                "memory live counter reports {tracked}, but {actual} allocation slots are occupied"
            ),
            _ => unreachable!("count-specific error formatting"),
        }
    }

    fn fmt_operation(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecoveryOutsideSelection => {
                f.write_str("recovery result created outside executing selection")
            }
            Self::ExpectedRecoveryResult => {
                f.write_str("expected a sealed recovery-selection result")
            }
            Self::ForeignRecoveryResult => {
                f.write_str("recovery result belongs to another selection or execution")
            }
            Self::RecoveryEscape => {
                f.write_str("recovery-selection result used as an ordinary value")
            }
            Self::RecoveryIdentityOverflow => f.write_str("recovery identity overflow"),
            Self::ExpectedCause => f.write_str("expected an owned counted cause"),
            Self::ExpectedDefect => f.write_str("expected a structured counted defect"),
            Self::ExpectedResult => {
                f.write_str("expected Prelude.Result.Ok or Err with exactly one ordinary field")
            }
            Self::ExpectedServiceTag => f.write_str("expected a nominal service tag"),
            Self::ExpectedLayer => f.write_str("expected a counted Layer"),
            Self::UnsupportedOperation(operation) => {
                write!(f, "counted execution does not yet support {operation}")
            }
            Self::MissingHandler { effect, op } => {
                write!(f, "no handler of {effect}.{op} is in effect")
            }
            Self::OperationArgumentCount {
                effect,
                op,
                expected_dicts,
                actual_dicts,
                expected_args,
                actual_args,
            } => write!(
                f,
                "operation {effect}.{op} expects {expected_dicts} dictionaries and {expected_args} arguments, got {actual_dicts} dictionaries and {actual_args} arguments"
            ),
            Self::ArgumentCount { expected, actual } => {
                write!(
                    f,
                    "memory function expects {expected} arguments, got {actual}"
                )
            }
            Self::HandlerIdentityOverflow => f.write_str("counted handler identity overflow"),
            Self::ExpectedDictionary => f.write_str("expected a counted dictionary"),
            Self::MissingMethod(name) => write!(f, "counted dictionary has no method {name}"),
            Self::ExpectedStructure => f.write_str("expected a record or exact tuple"),
            Self::StructureArity { expected, actual } => {
                write!(f, "structural view expects {expected} fields, got {actual}")
            }
            Self::PatternLayout { expected, actual } => write!(
                f,
                "pattern value layout expects {expected} fields, got {actual}"
            ),
            Self::IntDivisionByZero { remainder } => f.write_str(int_zero_message(*remainder)),
            Self::BigIntNegativeExponent => {
                f.write_str("RangeError: Exponent must be non-negative")
            }
            Self::BigIntNonIntegralNumber => {
                f.write_str("RangeError: BigInt requires a finite integer Number")
            }
            Self::BigIntInvalidString => f.write_str("SyntaxError: invalid BigInt integer string"),
            Self::BigIntUndefined => f.write_str("TypeError: cannot convert undefined to BigInt"),
            Self::ConstructorFieldOverflow => {
                f.write_str("constructor total field count overflows")
            }
            Self::ExpectedModifyPair => {
                f.write_str("Ref.modify callback must return (result, new contents)")
            }
            Self::InvalidTaggedBoundary { ordinary, physical } => write!(
                f,
                "tagged ordinary boundary {ordinary} exceeds physical field count {physical}"
            ),
            Self::ReservedConstructorEvidence(tag) => write!(
                f,
                "reserved constructor {tag} cannot carry dictionary evidence"
            ),
            Self::PatternEvidenceLayout {
                expected_fields,
                actual_fields,
                expected_dicts,
                actual_dicts,
            } => write!(
                f,
                "constructor pattern expects {expected_fields} ordinary fields and {expected_dicts} dictionaries, got {actual_fields} and {actual_dicts}"
            ),
            Self::UnknownGlobal(name) => {
                write!(f, "unknown memory entry {}.{}", name.module, name.name)
            }
            Self::GlobalCycle(name) => {
                write!(f, "cyclic memory initializer {}.{}", name.module, name.name)
            }
            Self::MissingField { object, field } => {
                write!(f, "object {object:?} has no field {field}")
            }
            _ => unreachable!("operation-specific error formatting"),
        }
    }

    fn fmt_control(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedVar => f.write_str("expected a frame-local variable handle"),
            Self::ForeignVar => f.write_str("variable belongs to another program execution"),
            Self::ClosedVar => f.write_str("frame-local variable is closed"),
            Self::ExpectedLazy => f.write_str("expected a Lazy handle"),
            Self::ForeignLazy => f.write_str("Lazy cell belongs to another program execution"),
            Self::ClosedLazy => f.write_str("Lazy cell is closed"),
            Self::LazyCycle => f.write_str("cyclic lazy initializer"),
            Self::ResumeTwice => f.write_str("`resume` ran twice"),
            Self::ForeignContinuation => {
                f.write_str("continuation belongs to another program execution")
            }
            Self::MissingLoopTarget(id) => write!(f, "no enclosing loop `{id}` is active"),
            Self::MissingFunctionBoundary => {
                f.write_str("Option `?` ran without an enclosing function boundary")
            }
            Self::MissingGeneratorBoundary => {
                f.write_str("`yield` ran without an enclosing generator boundary")
            }
            Self::ExpectedSeq => f.write_str("expected a List or Seq"),
            Self::ForeignSeq => f.write_str("Seq belongs to another program execution"),
            Self::ExpectedIterator => f.write_str("expected a sequence iterator handle"),
            Self::ForeignIterator => f.write_str("iterator belongs to another program execution"),
            Self::ClosedIterator => f.write_str("sequence iterator is closed"),
            Self::ExpectedLabel => f.write_str("expected a record label"),
            Self::ExpectedExit => f.write_str("expected an Exit value"),
            Self::ExpectedOption => f.write_str("expected Prelude.Option.Some or None"),
            Self::ExpectedInt => f.write_str("expected an Int"),
            Self::ExpectedFloat => f.write_str("expected a Float"),
            Self::Todo(msg) | Self::Assert(msg) => write!(f, "Error: {msg}"),
            Self::RangeError(msg) => write!(f, "RangeError: {msg}"),
            _ => unreachable!("control-flow error formatting"),
        }
    }
}

fn int_zero_message(remainder: bool) -> &'static str {
    if remainder {
        "RangeError: Int.unsafe_rem: division by zero"
    } else {
        "RangeError: Int.unsafe_div: division by zero"
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFunction(error) | Self::InvalidProgram(error) => error.fmt(f),
            Self::UnhandledFailure => f.write_str("unhandled typed failure"),
            Self::NeedsProgram => f.write_str("this memory operation requires a program execution"),
            Self::RecoveryOutsideSelection
            | Self::ExpectedRecoveryResult
            | Self::ForeignRecoveryResult
            | Self::RecoveryEscape
            | Self::RecoveryIdentityOverflow
            | Self::ExpectedCause
            | Self::ExpectedDefect
            | Self::ExpectedResult
            | Self::ExpectedServiceTag
            | Self::ExpectedLayer
            | Self::UnsupportedOperation(_)
            | Self::MissingHandler { .. }
            | Self::OperationArgumentCount { .. }
            | Self::HandlerIdentityOverflow
            | Self::ArgumentCount { .. }
            | Self::ExpectedDictionary
            | Self::MissingMethod(_)
            | Self::ExpectedStructure
            | Self::StructureArity { .. }
            | Self::PatternLayout { .. }
            | Self::IntDivisionByZero { .. }
            | Self::BigIntNegativeExponent
            | Self::BigIntNonIntegralNumber
            | Self::BigIntInvalidString
            | Self::BigIntUndefined
            | Self::ConstructorFieldOverflow
            | Self::ExpectedModifyPair
            | Self::InvalidTaggedBoundary { .. }
            | Self::ReservedConstructorEvidence(_)
            | Self::PatternEvidenceLayout { .. }
            | Self::UnknownGlobal(_)
            | Self::GlobalCycle(_)
            | Self::MissingField { .. } => self.fmt_operation(f),
            Self::NoMatch => f.write_str("no pattern matched"),
            Self::PatternKind(kind) => write!(f, "expected {kind} for pattern matching"),
            Self::OutOfFuel => f.write_str("counted program exhausted its execution fuel"),
            Self::ExpectedClosure => f.write_str("expected a counted closure"),
            Self::ForeignClosure => f.write_str("closure belongs to another program execution"),
            Self::ExpectedTask => f.write_str("expected a counted task"),
            Self::ForeignTask => f.write_str("task belongs to another program execution"),
            Self::ExpectedCell => f.write_str("expected a root-arena cell handle"),
            Self::ForeignCell => f.write_str("cell belongs to another program execution"),
            Self::ClosedCell => f.write_str("cell arena is closed"),
            Self::ExpectedVar
            | Self::ForeignVar
            | Self::ClosedVar
            | Self::ExpectedLazy
            | Self::ForeignLazy
            | Self::ClosedLazy
            | Self::LazyCycle
            | Self::ResumeTwice
            | Self::ForeignContinuation
            | Self::MissingLoopTarget(_)
            | Self::MissingFunctionBoundary
            | Self::MissingGeneratorBoundary
            | Self::ExpectedSeq
            | Self::ForeignSeq
            | Self::ExpectedIterator
            | Self::ForeignIterator
            | Self::ClosedIterator
            | Self::ExpectedLabel
            | Self::ExpectedExit
            | Self::ExpectedOption
            | Self::ExpectedInt
            | Self::ExpectedFloat
            | Self::Todo(_)
            | Self::Assert(_)
            | Self::RangeError(_) => self.fmt_control(f),
            Self::ExpectedOwner => f.write_str("expected a scoped owner handle"),
            Self::ForeignOwner => f.write_str("owner belongs to another execution or heap"),
            Self::ClosedOwner => f.write_str("scope owner is closed"),
            Self::ExpectedArena => f.write_str("expected an arena handle"),
            Self::ExpectedArenaView => f.write_str("expected an arena view"),
            Self::ForeignArena => f.write_str("arena belongs to another execution or heap"),
            Self::ClosedArena => f.write_str("arena is closed"),
            Self::MissingArenaSlot(index) => write!(f, "arena has no slot {index}"),
            Self::ExpectedRecord => f.write_str("expected a named memory record"),
            Self::MissingNamedField(name) => write!(f, "memory record has no field {name}"),
            Self::DuplicateField(name) => write!(f, "memory record repeats field {name}"),
            Self::UnsupportedPrimitive(what) => write!(f, "unsupported counted primitive: {what}"),
            Self::InvalidBigInt(text) => write!(f, "invalid counted BigInt literal {text}"),
            Self::InvalidObject(id) => write!(f, "invalid or released memory object {id:?}"),
            Self::ReferenceOverflow(id) => write!(f, "reference count overflow for {id:?}"),
            Self::InsufficientReferences { .. } | Self::LiveCountMismatch { .. } => {
                self.fmt_counts(f)
            }
            Self::ExpectedObject => f.write_str("expected a tagged memory object"),
            Self::ExpectedString => f.write_str("expected a memory string"),
            Self::ExpectedBigInt => f.write_str("expected a memory BigInt"),
            Self::ExpectedBoolean => f.write_str("expected an immediate Bool condition"),
            Self::NoToken(local) => write!(f, "memory local {local} has no owned token"),
            Self::Leftover(tokens) => write!(f, "unreleased memory tokens: {tokens:?}"),
            Self::LiveObjects(count) => write!(f, "{count} memory objects are still live"),
        }
    }
}

impl error::Error for Error {}
