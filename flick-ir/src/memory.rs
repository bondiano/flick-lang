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
mod foreign;
pub use foreign::{ForeignCallSpec, WireHint};
mod handlers;
pub use handlers::{HandlerEffect, HandlerOperation};
mod intrinsics;
pub use intrinsics::{HostPrimitive, Intrinsic};

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

/// Target-independent representation of a memory-IR value.
///
/// `Cell` is a heap-allocated reference-counted cell (or immediate scalar).
/// `Aggregate` is a flat by-value aggregate whose ownership is the multiset
/// of its boxed fields (`0` for immediate aggregates) and which has no heap
/// cell identity of its own, so it can never be a `DropReuse` or `Reuse*`
/// candidate.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum ValueRep {
    #[default]
    Cell,
    Aggregate,
}

impl ValueRep {
    pub fn is_cell(&self) -> bool {
        matches!(self, Self::Cell)
    }

    pub fn is_aggregate(&self) -> bool {
        matches!(self, Self::Aggregate)
    }
}

/// Target-independent heap-cell layout compatibility descriptor.
///
/// Distinguishes typed heap cells that share the same logical field count
/// while having incompatible storage layouts (for example, different inline vs.
/// boxed field shapes or alignment requirements) without committing to a
/// specific target ABI.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CellLayout {
    pub capacity: usize,
    pub align: u32,
    pub ownership: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operation {
    /// Strings and arbitrary-sized integers allocate; other literals are immediate.
    Literal(Lit),
    /// Takes ownership of every field, including repeated operands.
    Construct {
        tag: String,
        fields: Vec<Local>,
        #[serde(default, skip_serializing_if = "ValueRep::is_cell")]
        rep: ValueRep,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<CellLayout>,
    },
    /// Separate semantic payload from owned evidence. Physical projection
    /// order remains ordinary fields followed by dictionaries.
    ConstructWithDicts {
        tag: String,
        fields: Vec<Local>,
        dicts: Vec<Local>,
        #[serde(default, skip_serializing_if = "ValueRep::is_cell")]
        rep: ValueRep,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<CellLayout>,
    },
    /// Borrows the object and returns a new owned token for its field.
    Project {
        object: Local,
        field: usize,
        #[serde(default, skip_serializing_if = "ValueRep::is_cell")]
        rep: ValueRep,
    },
    /// Canonical UTF-16 label order after source-order field evaluation.
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
        pattern: Box<Pat>,
        #[serde(default, skip_serializing_if = "ValueRep::is_cell")]
        rep: ValueRep,
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
    /// Consumes an owned object at its last use and detaches a frame-limited
    /// reuse token when the object is uniquely owned (`references == 1`) and
    /// reusable (`Data::Tagged` or `Data::Record`). Drops old children
    /// immediately to preserve cleanup order and restore child uniqueness;
    /// otherwise drops the operand normally (leaving external aliases intact)
    /// and yields an immediate null reuse token.
    DropReuse(Local),
    /// Consumes the candidate (or a `DropReuse` token) and fields. Reuses the
    /// candidate's storage only if it is unique and has the same tagged field
    /// capacity and compatible layout. Otherwise drops it and allocates, with
    /// identical value semantics.
    Reuse {
        object: Local,
        tag: String,
        fields: Vec<Local>,
        #[serde(default, skip_serializing_if = "ValueRep::is_cell")]
        rep: ValueRep,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<CellLayout>,
    },
    /// Same physical-capacity/layout/uniqueness check as Reuse, retaining the
    /// explicit semantic payload/evidence boundary of the replacement.
    ReuseWithDicts {
        object: Local,
        tag: String,
        fields: Vec<Local>,
        dicts: Vec<Local>,
        #[serde(default, skip_serializing_if = "ValueRep::is_cell")]
        rep: ValueRep,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<CellLayout>,
    },
    /// Consumes the candidate (or a `DropReuse` token) and canonical record
    /// fields. Reuses the candidate's storage only if it is a unique record or
    /// record reuse token with the same field count; otherwise drops it and
    /// allocates a fresh record, preserving external aliases.
    ReuseRecord {
        object: Local,
        fields: Vec<(String, Local)>,
    },
    /// Consumes an unboxed aggregate value and transfers its owned references
    /// into a boxed fallback heap cell.
    BoxAggregate {
        value: Local,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<CellLayout>,
    },
    /// Borrows a boxed fallback cell and returns an owned aggregate copy,
    /// retaining every boxed reference inside it without releasing the cell.
    UnboxBorrow {
        object: Local,
    },
    /// Consumes a boxed fallback cell and returns an owned aggregate: transfers
    /// references without extra retains when unique, or retains references
    /// before releasing the cell when shared.
    UnboxConsume {
        object: Local,
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
    /// Allocates a frame-scoped mutable variable owned by the current frame.
    /// Consumes initial value; returns a non-owning Var handle released on frame exit/unwind.
    VarMake(Local),
    /// Consumes a non-owning Var handle and returns a retained owned value from its slot.
    VarGet(Local),
    /// Consumes a non-owning Var handle and replacement value; drops previous contents; returns Unit.
    VarSet {
        var: Local,
        value: Local,
    },
    /// Installs a lexical loop frame for `id` and invokes `code(args)` repeatedly until
    /// a matching `LoopExit` breaks it. Consumes `args`.
    Loop {
        id: String,
        code: CodeId,
        args: Vec<Local>,
    },
    /// Unwinds to the enclosing `Loop { id }` (closing departed scopes/handlers/vars):
    /// `None` starts the next iteration, `Some(value)` delivers `value` as the loop result.
    LoopExit {
        id: String,
        value: Option<Local>,
    },
    /// Marks a source-function return boundary for `UnwrapOption` (`Option?`).
    /// Runs static helper `code(args)`; synthetic branch/match helpers never emit this.
    FunctionBoundary {
        code: CodeId,
        args: Vec<Local>,
    },
    /// Consumes an `Option a`: returns owned `v` on `Some v`, or unwinds to the nearest
    /// `FunctionBoundary` (closing departed scopes) and delivers `Prelude.Option.None`.
    UnwrapOption(Local),
    /// Runs `code(args)` with a frame-owned list builder collecting `Yield` values; returns `$list`.
    GenerateStrict {
        code: CodeId,
        args: Vec<Local>,
    },
    /// Allocates a replayable lazy `Seq` owning `captures` and generator `code` (`[self, Unit]`),
    /// with optional element `limit`. Consumes `captures` and optional `limit`.
    GenerateLazy {
        code: CodeId,
        captures: Vec<Local>,
        limit: Option<Local>,
        sync: bool,
    },
    /// Yields an owned element to the innermost active `GenerateStrict` or lazy `Seq` iterator.
    Yield(Local),
    /// Consumes a list or `Seq`, opening a frame-owned iteration cursor (released on frame exit/unwind).
    SeqOpen(Local),
    /// Consumes a non-owning iterator cursor and yields `Prelude.Option.Some elem` or `None`.
    SeqNext(Local),
    /// First-class nominal record label value (`Record.label`).
    Label(String),
    /// Explicitly bridged foreign host call consuming `[dicts..., args..., optional_catch]`.
    /// Emitted only when the caller opts into foreign bridging; closed-world
    /// lowering never emits this operation.
    ForeignCall {
        spec: Box<ForeignCallSpec>,
        args: Vec<Local>,
    },
}

include!("memory/std_intrinsics.rs");
include!("memory/std_intrinsics_more.rs");
include!("memory/std_intrinsics_schema.rs");
include!("memory/std_intrinsics_ext.rs");
include!("memory/std_intrinsics_async.rs");
include!("memory/operations.rs");

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
    include!("memory/aggregate_tests.rs");
    include!("memory/serialization_tests.rs");
}
