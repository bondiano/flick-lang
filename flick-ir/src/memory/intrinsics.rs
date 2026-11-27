//! Platform-independent memory IR intrinsics and host primitives.

use serde::{Deserialize, Serialize};

use super::{
    ArbitraryIntrinsic, AsyncIntrinsic, BigDecimalIntrinsic, BigIntIntrinsic, BytesIntrinsic,
    ConfigIntrinsic, CronIntrinsic, CryptoIntrinsic, DateTimeIntrinsic, DebugIntrinsic,
    DurationIntrinsic, FixedWidthIntrinsic, FloatIntrinsic, GenIntrinsic, GraphIntrinsic,
    HashIntrinsic, HostServiceIntrinsic, IntIntrinsic, JsonIntrinsic, JsonSchemaIntrinsic,
    LayerIntrinsic, ListIntrinsic, OutcomeIntrinsic, PathIntrinsic, RecordIntrinsic,
    RegExpIntrinsic, SchemaIntrinsic, StringIntrinsic,
};

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
    /// Allocates an unsynchronized fiber-local `Lazy r e a` cell (O3) from a delayed task.
    LazyMakeLocal,
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
    /// Pure fixed-width integer and binary32 float primitives (`flick:rt`).
    FixedWidth(FixedWidthIntrinsic),
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
    /// Cooperative fiber, concurrency, deferred, and test-clock intrinsics (Stage 4).
    Async(AsyncIntrinsic),
    /// Dynamic `Layer`, `MemoMap`, and `Context` intrinsics (Stage 4).
    Layer(LayerIntrinsic),
    /// Pure metered cryptographic digest and PBKDF2 primitives (Stage 4D).
    Crypto(CryptoIntrinsic),
    /// Pure metered `Config` and `ConfigProvider` helper primitives (Stage 4D).
    Config(ConfigIntrinsic),
    /// Pure metered `Arbitrary` RNG, shrink, and formatting primitives (Stage 4D).
    Arbitrary(ArbitraryIntrinsic),
    /// Native Rust VM host service operations (`File`, `FileSystem`, `ChildProcess`).
    HostService(HostServiceIntrinsic),
    /// Immutable command construction and host execution for the audited CLI ABI.
    Cli(CliIntrinsic),
}

/// The owned operand order is the original binding order, including class
/// dictionaries preceding the source parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CliIntrinsic {
    Command,
    Description,
    /// `Cli.Value`, `Record.Lacks`, `Record.UnionDisjoint`, label, aliases, help, multi, spec.
    Flag,
    /// `Cli.Value`, `Record.Lacks`, `Record.UnionDisjoint`, label, help, multi, spec.
    Arg,
    Subcommand,
    Handle,
    Group,
    Required,
    Optional,
    Many,
    RunVersion,
}

impl CliIntrinsic {
    pub const fn arity(self) -> usize {
        match self {
            Self::Command | Self::Group => 1,
            Self::Description | Self::Handle => 2,
            Self::Subcommand | Self::RunVersion => 3,
            Self::Flag => 8,
            Self::Arg => 7,
            Self::Required | Self::Optional | Self::Many => 0,
        }
    }
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
