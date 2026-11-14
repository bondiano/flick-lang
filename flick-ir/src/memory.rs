//! Explicit ownership for the platform-independent memory lowering.
//!
//! This is an ANF memory IR, not an annotation on syntax-tree sharing. A
//! local owns one token; `Dup` creates another and `Drop` consumes one.
//! Operations consume their operands except for the explicitly borrowed
//! receivers of `Project`, `Field` and `Match`. Only the selected branch executes.
//!
//! Source-to-memory lowering is separate from this representation. In
//! particular, a serialized typed [`crate::Program`] does not yet contain
//! these blocks. See `docs/formal/memory.md` for the implementation boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::{error, fmt, iter};

use serde::{Deserialize, Serialize};

use crate::{BinOp, Lit, Num, Pat};

mod pattern;
pub use pattern::pattern_bindings;
mod handlers;
pub use handlers::{HandlerEffect, HandlerOperation};

pub type Local = u32;
pub type CodeId = u32;

/// Qualified identities stay separate; generated names may contain dots.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Name {
    pub module: String,
    pub name: String,
}

impl Name {
    pub fn new(module: &str, name: &str) -> Self {
        Self {
            module: module.into(),
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Global {
    pub name: Name,
    /// A zero-parameter initializer, evaluated once per execution.
    pub initializer: CodeId,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub functions: BTreeMap<CodeId, Function>,
    /// A vector avoids encoding structured identities as JSON map keys.
    pub globals: Vec<Global>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    /// One owned token per parameter, even when two arguments alias.
    pub params: Vec<Local>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub instructions: Vec<Instruction>,
    pub exit: Exit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    Let { binding: Local, value: Operation },
    Dup(Local),
    Drop(Local),
}

/// An eager value or a unary getter called with the actual dictionary.
/// Getters own their captures, never an unfinished dictionary self. Lookup
/// does not cache their results: a returned method may itself own self.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DictionaryEntry {
    Value(Local),
    Getter(Local),
}

impl DictionaryEntry {
    pub fn local(&self) -> Local {
        match self {
            Self::Value(local) | Self::Getter(local) => *local,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operation {
    /// Strings and arbitrary-sized integers allocate; other literals are immediate.
    Literal(Lit),
    /// Takes ownership of every field, including repeated operands.
    Construct {
        tag: String,
        fields: Vec<Local>,
    },
    /// Separate semantic payload from owned evidence. Physical projection
    /// order remains ordinary fields followed by dictionaries.
    ConstructWithDicts {
        tag: String,
        fields: Vec<Local>,
        dicts: Vec<Local>,
    },
    /// Borrows the object and returns a new owned token for its field.
    Project {
        object: Local,
        field: usize,
    },
    /// Label order is evaluation order, not an inferred positional layout.
    Record {
        fields: Vec<(String, Local)>,
    },
    /// Like Project, borrows its receiver and returns an owned field token.
    Field {
        object: Local,
        field: String,
    },
    /// Borrows its receiver. Returns Tagged "$match": failure [false],
    /// success [true, bindings...] in shared `pattern_bindings` lexical
    /// order. Selection itself creates no owned aliases or list tails.
    Match {
        object: Local,
        pattern: Pat,
    },
    /// Consumes the base and every replacement in written order, including
    /// duplicate labels. Produces a fresh record; the last duplicate wins.
    Update {
        object: Local,
        fields: Vec<(String, Local)>,
    },
    /// Owns its captures, but never a self-owning field. The code takes
    /// exactly [owned closure self, owned argument]; captures are projected
    /// from self. Each source arrow is a separate unary closure stage.
    Closure {
        code: CodeId,
        captures: Vec<Local>,
    },
    /// A distinct delayed value, not an ordinary callable closure. Its
    /// code receives [owned task self, Unit] and runs anew on each await.
    Task {
        code: CodeId,
        captures: Vec<Local>,
    },
    Await(Local),
    /// Moves closure self and argument into its unary code frame.
    Apply {
        function: Local,
        argument: Local,
    },
    /// Static helper call, transferring one owned token per parameter.
    Call {
        code: CodeId,
        args: Vec<Local>,
    },
    /// Runs a static helper under a selected live owner, restoring the
    /// caller owner when it returns. The body is not evaluated eagerly.
    Within {
        owner: Local,
        code: CodeId,
        args: Vec<Local>,
    },
    /// Reads the inert candidate, returning a nonowning Owner or Unit.
    ReadOwnerEvidence,
    /// Consumes candidate and arguments, running a lazy helper with only
    /// lexical evidence changed. Does not validate, select or close an owner.
    WithOwnerEvidence {
        owner: Local,
        code: CodeId,
        args: Vec<Local>,
    },
    /// Retains a value from the execution's explicitly owned global cache.
    Global(Name),
    Binary {
        op: BinOp,
        left: Local,
        right: Local,
    },
    Not(Local),
    Neg {
        num: Num,
        value: Local,
    },
    Intrinsic {
        kind: Intrinsic,
        args: Vec<Local>,
    },
    /// Consumes the candidate and fields. Reuses the candidate's storage
    /// only if it is unique and has the same field capacity. Otherwise
    /// drops it and allocates, with exactly the same value semantics.
    Reuse {
        object: Local,
        tag: String,
        fields: Vec<Local>,
    },
    /// Same physical-capacity/uniqueness check as Reuse, retaining the
    /// explicit semantic payload/evidence boundary of the replacement.
    ReuseWithDicts {
        object: Local,
        tag: String,
        fields: Vec<Local>,
        dicts: Vec<Local>,
    },
    /// Installs operation arms around a delayed body called with Unit.
    /// Owns body, then operation functions in metadata order, then optional
    /// return and error-dispatch closures. Arms execute outside the boundary.
    Handle {
        body: Local,
        effects: Vec<HandlerEffect>,
        ret: Option<Local>,
        errors: Option<Local>,
    },
    /// A saturated operation at its first runtime stage. Dictionaries are
    /// consumed before ordinary arguments; lookup uses the execution context,
    /// not the context where the operation was partially applied.
    Perform {
        effect: String,
        op: String,
        dicts: Vec<Local>,
        args: Vec<Local>,
    },
    /// An owned opaque nominal service tag. The key is compiler metadata,
    /// not a user String value; live tags for one key are canonical per heap.
    ServiceTag(String),
    /// Owns entries in written evaluation order. Distinct from a record:
    /// ordinary field/projection/pattern operations never force getters.
    Dictionary {
        entries: Vec<(String, DictionaryEntry)>,
    },
    /// Borrows the dictionary and returns an owned value. A getter runs
    /// with the actual receiver under the lookup caller's execution context.
    Method {
        dictionary: Local,
        field: String,
    },
    /// Borrows a record or exact tuple and returns an owned tuple containing
    /// `(is_record, tuple_values, tuple_labels)`. Record fields are ordered by
    /// UTF-16 label order; tuple labels are Unit. Shape/count must match.
    Structure {
        object: Local,
        expected: usize,
    },
    /// Sealed recovery selection: None misses; Some owns a deferred Unit closure.
    /// It is not an ordinary source value and may only return through selector helpers.
    RecoveryResult {
        selected: Option<Local>,
    },
}

/// Platform-independent operations whose host bindings have a known
/// ownership contract. Unknown foreign bindings are not silently accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intrinsic {
    PrintLine,
    Show,
    RefMake,
    RefGet,
    RefSet,
    RefModify,
    RefSame,
    ScopeRun,
    ScopeCurrent,
    ArenaMake,
    ArenaAlloc,
    ArenaGet,
    /// No-argument failure for a match with no selected arm; never returns.
    MatchFail,
    /// Calls the body with Unit and catches typed raises only.
    Try,
    /// Unwraps an exact Prelude.Result or raises its Err payload.
    FromResult,
    /// Raises an owned typed payload; never returns.
    Raise,
    /// Consumes an opaque tag and implementation into an owned constant Layer.
    LayerSucceed,
    /// Consumes a Layer and callback, supplies Unit under a complete provider,
    /// then closes that run's owner before delivering the callback outcome.
    LayerRun,
    /// Subject-last, exact string prefix predicate; consumes two owned inputs.
    StringStartsWith,
    /// Subject-last, exact string substring predicate; consumes two owned inputs.
    StringIncludes,
    /// Total primitive comparison (NaN last), returning Prelude.Ordering.
    PrimitiveCompare,
    /// Canonical rt/Go primitive hash, including exact Ordering nullaries.
    PrimitiveHash,
    /// Consumes two Layer roots into a shared graph; later services win.
    LayerMerge,
    /// Signed Int quotient truncated toward zero; zero divisor is a defect.
    IntUnsafeDiv,
    /// Signed Int remainder; zero divisor is a defect, MIN % -1 is zero.
    IntUnsafeRem,
    /// Number of UTF-16 code units, consuming one owned string token.
    StringLength,
    /// Scalar global Number conversions from Int, `BigInt` or Float.
    NumberToFloat,
    /// Pinned Number.min(that)(self): NaN least, equality selects self.
    FloatMin,
    /// Pinned Number.max(that)(self): NaN least, equality selects self.
    FloatMax,
    /// Shallow readonly array ABI normalization: list identity or tuple fields
    /// retained into a list. Other representations are rejected.
    ArrayToList,
    /// Audited global `BigInt` conversion over checked language primitives.
    ToBigInt,
}

impl Intrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::MatchFail => 0,
            Self::PrintLine
            | Self::Show
            | Self::RefMake
            | Self::RefGet
            | Self::ScopeRun
            | Self::ScopeCurrent
            | Self::ArenaMake
            | Self::ArenaGet
            | Self::FromResult
            | Self::Raise
            | Self::PrimitiveHash
            | Self::StringLength
            | Self::NumberToFloat
            | Self::ArrayToList
            | Self::ToBigInt => 1,
            Self::RefSet
            | Self::RefModify
            | Self::RefSame
            | Self::ArenaAlloc
            | Self::Try
            | Self::LayerSucceed
            | Self::LayerRun
            | Self::LayerMerge
            | Self::StringStartsWith
            | Self::StringIncludes
            | Self::PrimitiveCompare
            | Self::IntUnsafeDiv
            | Self::IntUnsafeRem
            | Self::FloatMin
            | Self::FloatMax => 2,
        }
    }
}

impl Operation {
    pub fn consumed(&self) -> Vec<Local> {
        match self {
            Self::Literal(_)
            | Self::ReadOwnerEvidence
            | Self::Project { .. }
            | Self::Field { .. }
            | Self::Method { .. }
            | Self::Structure { .. }
            | Self::Match { .. }
            | Self::Global(_)
            | Self::ServiceTag(_) => Vec::new(),
            Self::RecoveryResult { selected } => selected.iter().copied().collect(),
            Self::Construct { fields, .. } => fields.clone(),
            Self::ConstructWithDicts { fields, dicts, .. } => {
                fields.iter().chain(dicts).copied().collect()
            }
            Self::Record { fields } => fields.iter().map(|(_, local)| *local).collect(),
            Self::Dictionary { entries } => {
                entries.iter().map(|(_, entry)| entry.local()).collect()
            }
            Self::Update { object, fields } => iter::once(*object)
                .chain(fields.iter().map(|(_, local)| *local))
                .collect(),
            Self::Closure { captures, .. } | Self::Task { captures, .. } => captures.clone(),
            Self::Call { args, .. } | Self::Intrinsic { args, .. } => args.clone(),
            Self::Handle {
                body,
                effects,
                ret,
                errors,
            } => iter::once(*body)
                .chain(
                    effects
                        .iter()
                        .flat_map(|effect| effect.operations.iter().map(|op| op.function)),
                )
                .chain(*ret)
                .chain(*errors)
                .collect(),
            Self::Perform { dicts, args, .. } => dicts.iter().chain(args).copied().collect(),
            Self::Within { owner, args, .. } | Self::WithOwnerEvidence { owner, args, .. } => {
                let mut operands = vec![*owner];
                operands.extend(args);
                operands
            }
            Self::Apply { function, argument } => vec![*function, *argument],
            Self::Binary { left, right, .. } => vec![*left, *right],
            Self::Not(value) | Self::Neg { value, .. } | Self::Await(value) => vec![*value],
            Self::ReuseWithDicts {
                object,
                fields,
                dicts,
                ..
            } => iter::once(*object)
                .chain(fields.iter().copied())
                .chain(dicts.iter().copied())
                .collect(),
            Self::Reuse { object, fields, .. } => {
                let mut operands = vec![*object];
                operands.extend(fields);
                operands
            }
        }
    }

    pub fn borrowed(&self) -> Option<Local> {
        match self {
            Self::Project { object, .. }
            | Self::Field { object, .. }
            | Self::Structure { object, .. }
            | Self::Match { object, .. } => Some(*object),
            Self::Method { dictionary, .. } => Some(*dictionary),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Exit {
    /// Transfers one owned token to the caller, which must release it.
    Return(Local),
    /// Consumes the immediate Bool condition, then transfers the remaining
    /// tokens to exactly one successor. Branches do not copy the heap.
    Branch {
        condition: Local,
        then_: Box<Block>,
        else_: Box<Block>,
    },
}

include!("memory/errors.rs");

include!("memory/validation.rs");
#[cfg(test)]
mod tests {
    use super::*;

    include!("memory/validation_tests.rs");
    include!("memory/serialization_tests.rs");
}
