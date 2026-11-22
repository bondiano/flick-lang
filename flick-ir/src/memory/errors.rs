#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    DuplicatePatternBinding(String),
    EmptyOrPattern,
    UnequalOrBindings,
    DuplicatePatternField(String),
    ConstructorPatternArity {
        expected: usize,
        actual: usize,
    },
    PatternArityOverflow,
    ConstructorFieldOverflow,
    ReservedConstructorEvidence(String),
    DuplicateBinding(Local),
    NoToken(Local),
    Leftover(Vec<(Local, usize)>),
    UnknownCode(CodeId),
    UnknownGlobal(Name),
    DuplicateGlobal(Name),
    Arity {
        code: CodeId,
        expected: usize,
        actual: usize,
    },
    IntrinsicArity {
        kind: Intrinsic,
        actual: usize,
    },
    ForeignArity {
        name: String,
        expected: usize,
        actual: usize,
    },
    DuplicateHandledEffect(String),
    DuplicateHandledOperation {
        effect: String,
        op: String,
    },
    HandlerArityOverflow {
        effect: String,
        op: String,
    },
    InCode {
        code: CodeId,
        error: Box<Error>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePatternBinding(name) => write!(f, "pattern repeats binder {name}"),
            Self::EmptyOrPattern => f.write_str("Or pattern has no alternatives"),
            Self::UnequalOrBindings => f.write_str("Or pattern alternatives bind different names"),
            Self::DuplicatePatternField(name) => write!(f, "record pattern repeats field {name}"),
            Self::ConstructorPatternArity { expected, actual } => write!(
                f,
                "constructor pattern expects {expected} fields including dictionaries, got {actual}"
            ),
            Self::ConstructorFieldOverflow => {
                f.write_str("constructor total field count overflows")
            }
            Self::ReservedConstructorEvidence(tag) => write!(
                f,
                "reserved constructor {tag} cannot carry dictionary evidence"
            ),
            Self::PatternArityOverflow => f.write_str("constructor pattern field count overflows"),
            Self::DuplicateBinding(local) => write!(f, "memory local {local} is bound twice"),
            Self::NoToken(local) => write!(f, "memory local {local} has no owned token"),
            Self::Leftover(tokens) => write!(f, "unreleased memory tokens: {tokens:?}"),
            Self::UnknownCode(code) => write!(f, "unknown memory code {code}"),
            Self::UnknownGlobal(name) => {
                write!(f, "unknown memory global {}.{}", name.module, name.name)
            }
            Self::DuplicateGlobal(name) => write!(
                f,
                "memory global {}.{} is bound twice",
                name.module, name.name
            ),
            Self::Arity {
                code,
                expected,
                actual,
            } => write!(
                f,
                "memory code {code} expects {expected} parameters, got {actual}"
            ),
            Self::IntrinsicArity { kind, actual } => write!(
                f,
                "memory intrinsic {kind:?} expects {} arguments, got {actual}",
                kind.arity()
            ),
            Self::ForeignArity {
                name,
                expected,
                actual,
            } => write!(
                f,
                "memory foreign call {name} expects {expected} arguments, got {actual}"
            ),
            Self::DuplicateHandledEffect(effect) => write!(f, "handler repeats effect {effect}"),
            Self::DuplicateHandledOperation { effect, op } => {
                write!(f, "handler repeats operation {effect}.{op}")
            }
            Self::HandlerArityOverflow { effect, op } => write!(
                f,
                "handler operation {effect}.{op} invocation arity overflows"
            ),
            Self::InCode { code, error } => write!(f, "memory code {code}: {error}"),
        }
    }
}

impl error::Error for Error {}
