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
    ExpectedContext,
    ExpectedContextRef,
    ExpectedFiber,
    ExpectedDeferred,
    ExpectedMemoMap,
    ExpectedFiberGroup,
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
    HostBridge(String),
    LeakedForeignHandles(usize),
}

impl Error {
    fn expected_message(&self) -> Option<&'static str> {
        Some(match self {
            Self::ExpectedRecoveryResult => "expected a sealed recovery-selection result",
            Self::ForeignRecoveryResult => {
                "recovery result belongs to another selection or execution"
            }
            Self::ExpectedCause => "expected an owned counted cause",
            Self::ExpectedDefect => "expected a structured counted defect",
            Self::ExpectedResult => {
                "expected Prelude.Result.Ok or Err with exactly one ordinary field"
            }
            Self::ExpectedServiceTag => "expected a nominal service tag",
            Self::ExpectedLayer => "expected a counted Layer",
            Self::ExpectedContext => "expected a counted Context",
            Self::ExpectedContextRef => "expected a Context.Reference",
            Self::ExpectedFiber => "expected a counted Fiber handle",
            Self::ExpectedDeferred => "expected a counted Deferred handle",
            Self::ExpectedMemoMap => "expected a counted Layer.MemoMap",
            Self::ExpectedFiberGroup => "expected a counted FiberSet/Map/Handle",
            Self::ExpectedDictionary => "expected a counted dictionary",
            Self::ExpectedStructure => "expected a record or exact tuple",
            Self::ExpectedModifyPair => "Ref.modify callback must return (result, new contents)",
            Self::ExpectedClosure => "expected a counted closure",
            Self::ForeignClosure => "closure belongs to another program execution",
            Self::ExpectedTask => "expected a counted task",
            Self::ForeignTask => "task belongs to another program execution",
            Self::ExpectedCell => "expected a root-arena cell handle",
            Self::ForeignCell => "cell belongs to another program execution",
            Self::ClosedCell => "cell arena is closed",
            Self::ExpectedVar => "expected a frame-local variable handle",
            Self::ForeignVar => "variable belongs to another program execution",
            Self::ClosedVar => "frame-local variable is closed",
            Self::ExpectedLazy => "expected a Lazy handle",
            Self::ForeignLazy => "Lazy cell belongs to another program execution",
            Self::ClosedLazy => "Lazy cell is closed",
            Self::ForeignContinuation => "continuation belongs to another program execution",
            Self::ExpectedSeq => "expected a List or Seq",
            Self::ForeignSeq => "Seq belongs to another program execution",
            Self::ExpectedIterator => "expected a sequence iterator handle",
            Self::ForeignIterator => "iterator belongs to another program execution",
            Self::ClosedIterator => "sequence iterator is closed",
            Self::ExpectedLabel => "expected a record label",
            Self::ExpectedExit => "expected an Exit value",
            Self::ExpectedOption => "expected Prelude.Option.Some or None",
            Self::ExpectedInt => "expected an Int",
            Self::ExpectedFloat => "expected a Float",
            Self::ExpectedOwner => "expected a scoped owner handle",
            Self::ForeignOwner => "owner belongs to another execution or heap",
            Self::ClosedOwner => "scope owner is closed",
            Self::ExpectedArena => "expected an arena handle",
            Self::ExpectedArenaView => "expected an arena view",
            Self::ForeignArena => "arena belongs to another execution or heap",
            Self::ClosedArena => "arena is closed",
            Self::ExpectedRecord => "expected a named memory record",
            Self::ExpectedObject => "expected a tagged memory object",
            Self::ExpectedString => "expected a memory string",
            Self::ExpectedBigInt => "expected a memory BigInt",
            Self::ExpectedBoolean => "expected an immediate Bool condition",
            _ => return None,
        })
    }

    fn static_message(&self) -> Option<&'static str> {
        Some(match self {
            Self::UnhandledFailure => "unhandled typed failure",
            Self::NeedsProgram => "this memory operation requires a program execution",
            Self::RecoveryOutsideSelection => "recovery result created outside executing selection",
            Self::RecoveryEscape => "recovery-selection result used as an ordinary value",
            Self::RecoveryIdentityOverflow => "recovery identity overflow",
            Self::HandlerIdentityOverflow => "counted handler identity overflow",
            Self::IntDivisionByZero { remainder } => int_zero_message(*remainder),
            Self::BigIntNegativeExponent => "RangeError: Exponent must be non-negative",
            Self::BigIntNonIntegralNumber => "RangeError: BigInt requires a finite integer Number",
            Self::BigIntInvalidString => "SyntaxError: invalid BigInt integer string",
            Self::BigIntUndefined => "TypeError: cannot convert undefined to BigInt",
            Self::ConstructorFieldOverflow => "constructor total field count overflows",
            Self::NoMatch => "no pattern matched",
            Self::OutOfFuel => "counted program exhausted its execution fuel",
            Self::LazyCycle => "cyclic lazy initializer",
            Self::ResumeTwice => "`resume` ran twice",
            Self::MissingFunctionBoundary => {
                "Option `?` ran without an enclosing function boundary"
            }
            Self::MissingGeneratorBoundary => "`yield` ran without an enclosing generator boundary",
            _ => return None,
        })
    }

    fn fmt_counts(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::PatternEvidenceLayout {
                expected_fields,
                actual_fields,
                expected_dicts,
                actual_dicts,
            } => write!(
                f,
                "constructor pattern expects {expected_fields} ordinary fields and {expected_dicts} dictionaries, got {actual_fields} and {actual_dicts}"
            ),
            Self::InsufficientReferences {
                object,
                required,
                actual,
            } => write!(
                f,
                "object {object:?} has {actual} references, needs {required}"
            ),
            Self::LiveCountMismatch { tracked, actual } => write!(
                f,
                "memory live counter reports {tracked}, but {actual} allocation slots are occupied"
            ),
            _ => unreachable!("count-specific error formatting"),
        }
    }

    fn fmt_dynamic(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFunction(error) | Self::InvalidProgram(error) => {
                fmt::Display::fmt(error, f)
            }
            Self::UnsupportedOperation(operation) => {
                write!(f, "counted execution does not yet support {operation}")
            }
            Self::MissingHandler { effect, op } => {
                write!(f, "no handler of {effect}.{op} is in effect")
            }
            Self::OperationArgumentCount { .. }
            | Self::PatternEvidenceLayout { .. }
            | Self::InsufficientReferences { .. }
            | Self::LiveCountMismatch { .. } => self.fmt_counts(f),
            Self::ArgumentCount { expected, actual } => write!(
                f,
                "memory function expects {expected} arguments, got {actual}"
            ),
            Self::MissingMethod(name) => write!(f, "counted dictionary has no method {name}"),
            Self::StructureArity { expected, actual } => {
                write!(f, "structural view expects {expected} fields, got {actual}")
            }
            Self::PatternKind(kind) => write!(f, "expected {kind} for pattern matching"),
            Self::PatternLayout { expected, actual } => write!(
                f,
                "pattern value layout expects {expected} fields, got {actual}"
            ),
            Self::InvalidTaggedBoundary { ordinary, physical } => write!(
                f,
                "tagged ordinary boundary {ordinary} exceeds physical field count {physical}"
            ),
            Self::ReservedConstructorEvidence(tag) => write!(
                f,
                "reserved constructor {tag} cannot carry dictionary evidence"
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
            Self::MissingLoopTarget(id) => write!(f, "no enclosing loop `{id}` is active"),
            Self::Todo(msg) | Self::Assert(msg) => write!(f, "Error: {msg}"),
            Self::RangeError(msg) => write!(f, "RangeError: {msg}"),
            Self::MissingArenaSlot(index) => write!(f, "arena has no slot {index}"),
            Self::MissingNamedField(name) => write!(f, "memory record has no field {name}"),
            Self::DuplicateField(name) => write!(f, "memory record repeats field {name}"),
            Self::UnsupportedPrimitive(what) => write!(f, "unsupported counted primitive: {what}"),
            Self::InvalidBigInt(text) => write!(f, "invalid counted BigInt literal {text}"),
            Self::InvalidObject(id) => write!(f, "invalid or released memory object {id:?}"),
            Self::ReferenceOverflow(id) => write!(f, "reference count overflow for {id:?}"),
            Self::NoToken(local) => write!(f, "memory local {local} has no owned token"),
            Self::Leftover(tokens) => write!(f, "unreleased memory tokens: {tokens:?}"),
            Self::LiveObjects(count) => write!(f, "{count} memory objects are still live"),
            Self::HostBridge(msg) => write!(f, "host bridge error: {msg}"),
            Self::LeakedForeignHandles(count) => {
                write!(f, "{count} host foreign handles are still live")
            }
            _ => unreachable!("static error formatted earlier"),
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
        if let Some(message) = self.expected_message() {
            return f.write_str(message);
        }
        if let Some(message) = self.static_message() {
            return f.write_str(message);
        }
        self.fmt_dynamic(f)
    }
}

impl error::Error for Error {}
