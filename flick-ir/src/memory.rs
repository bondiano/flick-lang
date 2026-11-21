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
    /// A primitive of the host that `Host.live` adapts; it takes one argument.
    Host(HostPrimitive),
    /// O(N) counted Int range `[from, ..., until - 1]`.
    ListRange,
    /// Allocates a single-flight `Lazy r e a` cell from a delayed task.
    LazyMake,
    /// Forces a `Lazy r e a` cell once, caching Ok/Err and dropping the task on completion.
    LazyForce,
    /// Registers a LIFO `Eff r none Unit` finalizer on the current owner.
    ScopeAddFinalizer,
    /// Registers a LIFO `Option RawCause -> Unit` finalizer on the current owner.
    ScopeAddFinalizerExit,
    /// Registers a LIFO `Exit e b -> Unit` close callback on the current owner.
    ScopeAddClose,
    /// Runs `(acquire, release)` under the current owner, registering LIFO release.
    AcquireRelease,
    /// Runs `(acquire, use, release)` guaranteeing release runs even on failure/defect.
    Bracket,
    /// Runs a task uninterruptibly (masks interruption around acquisition/registration).
    Uninterruptible,
    /// Row primitives for `Record.fl`.
    RecordGet,
    RecordExtend,
    RecordRemove,
    RecordProject,
    RecordMergeDisjoint,
    RecordLabels,
    /// Wraps a list/slice as a replayable `Seq`.
    SeqOfList,
    /// Drains a `Seq` or list into an owned `$list`.
    SeqToList,
    /// `Exit` observers used by `Scope.defer` / `Resource.make` / `Resource.own`.
    ExitIsSuccess,
    ExitIsFailure,
    ExitHasInterrupts,
    /// Pure `Int` primitives (`packages/core/src/Int.fl`).
    Int(IntIntrinsic),
    /// Pure `Float` / `Math` / `Number` primitives (`packages/core/src/Float.fl`).
    Float(FloatIntrinsic),
    /// Pure `BigInt` primitives (`packages/std/src/BigInt.fl`).
    BigInt(BigIntIntrinsic),
    /// Pure `String` primitives (`packages/core/src/String.fl`).
    String(StringIntrinsic),
    /// Pure non-callback `List` and `NonEmpty` primitives.
    List(ListIntrinsic),
    /// Pure non-callback `ReadonlyRecord` dictionary primitives.
    Record(RecordIntrinsic),
    /// `Flick.Debug` (`todo`, `echo`, `let assert`) intrinsics.
    Debug(DebugIntrinsic),
    /// Packed immutable `Bytes` primitives (`packages/std/src/Bytes.fl`).
    Bytes(BytesIntrinsic),
    /// Hash combining and HAMT walk-order primitives (`packages/std/src/Hash.fl`).
    Hash(HashIntrinsic),
    /// Pure JSON, JSON Pointer, JSON Patch, Formatter, Inspectable, and Version intrinsics.
    Json(JsonIntrinsic),
    /// Exact decimal primitives (`packages/std/src/BigDecimal.fl`).
    BigDecimal(BigDecimalIntrinsic),
    /// Immutable regular expression primitives (`packages/std/src/RegExp.fl`).
    RegExp(RegExpIntrinsic),
    /// Non-negative `Duration` primitives (`packages/std/src/Duration.fl`).
    Duration(DurationIntrinsic),
    /// Pure `Exit` / `Cause` / `Failure` primitives.
    Outcome(OutcomeIntrinsic),
    /// Pure `Gen` PRNG step, ASCII string draw, and non-empty check primitives.
    Gen(GenIntrinsic),
    /// Pure `Schema`, `Codec`, `Check`, `Route`, and `Differ.json_patch` primitives.
    Schema(SchemaIntrinsic),
    /// Pure `JsonSchema` dialect conversion primitives.
    JsonSchema(JsonSchemaIntrinsic),
    /// Pure POSIX `Path` primitives (`packages/std/src/Path.fl`).
    Path(PathIntrinsic),
    /// Pure `DateTime` primitives (`packages/std/src/DateTime.fl`).
    DateTime(DateTimeIntrinsic),
    /// Pure `Cron` primitives (`packages/std/src/Cron.fl`).
    Cron(CronIntrinsic),
    /// Pure `Graph` primitives (`packages/std/src/Graph/*.fl`).
    Graph(GraphIntrinsic),
    /// Runs a task and captures any return or failure/defect/interruption into `Exit`.
    EffectExit,
    /// Unwinds immediately with a string defect message (`Effect.die`).
    EffectDie,
    /// Runs a task and recovers any defect via `handler(message)`, letting typed errors pass.
    EffectCatchDefect,
    /// Rethrows an owned `Failure` cause unchanged (`Failure.rethrow`).
    FailureRethrow,
}

/// The host primitives of `Host.live` besides `PrintLine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostPrimitive {
    /// Writes a line to stderr; borrows the text and returns Unit.
    StderrLine,
    /// Writes text to stdout without a line break; returns Unit.
    StdoutWrite,
    /// The host's environment variable of a name, as an `Option String`.
    Env,
    /// The host's environment variables of Unit, as a `List (String, String)`.
    Vars,
    /// The host's arguments of Unit, as a `List String`.
    Args,
    /// The host's wall clock of Unit, in milliseconds as a Float.
    CurrentTimeMillis,
    /// The host's wall clock of Unit, in nanoseconds as a `BigInt`.
    CurrentTimeNanos,
    /// The host's monotonic clock of Unit, in nanoseconds as a `BigInt`.
    MonotonicTimeNanos,
    /// A draw of the host's random source of Unit, a Float in [0, 1).
    RandomNext,
    /// A child process the host starts for a command, in the caller's scope.
    ProcessSpawn,
}

include!("memory/std_intrinsics.rs");
include!("memory/std_intrinsics_more.rs");
include!("memory/std_intrinsics_schema.rs");
include!("memory/std_intrinsics_ext.rs");
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
    include!("memory/serialization_tests.rs");
}
