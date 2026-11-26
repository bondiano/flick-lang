#[derive(Debug)]
enum LayerData {
    Succeed { tag: Value, implementation: Value },
    // Strong graph edges: duplicate roots retain their independent tokens.
    Merge { left: Value, right: Value },
    /// Constant empty layer (`Layer.empty`).
    Empty,
    /// Constant layer built from an existing `Data::Context` (`Layer.succeed_context`).
    Context(Value),
    /// Dynamic layer built by calling `build(memo_map)` in a fresh/memoized scope.
    Dynamic { memo: bool, build: Value },
    /// Builds `inner` with a fresh unshared `MemoMap` (`Layer.fresh`).
    Fresh(Value),
    /// Builds every layer concurrently, later services replacing earlier ones.
    MergeAll(Vec<Value>),
    /// Builds `inner`, turning typed build failures into defects (`Layer.or_die`).
    OrDie(Value),
}

#[derive(Debug)]
enum DictionaryValue {
    Value(Value),
    Getter(Value),
}

impl DictionaryValue {
    fn value(&self) -> &Value {
        match self {
            Self::Value(value) | Self::Getter(value) => value,
        }
    }

    fn into_value(self) -> Value {
        match self {
            Self::Value(value) | Self::Getter(value) => value,
        }
    }
}

#[derive(Debug)]
enum LazyState {
    Pending(Value),
    /// Retains the original task and owning fiber until completion so a
    /// defect/abort/signal restores Pending or hands off to waiters cleanly (`#626`).
    Running {
        task: Value,
        owner: usize,
    },
    /// Unsynchronized fiber-local cell (`LazyMakeLocal`, O3): no waiters or
    /// cross-fiber ownership. `task` is `None` when no enclosing abort or
    /// interrupt catcher can observe a reset, so Perceus frees the task and its
    /// captures on `Pending -> Running`; `Some(task)` when a handler/catcher on
    /// the stack may catch an abort/interrupt and force the cell again.
    RunningLocal {
        task: Option<Value>,
    },
    DoneOk(Value),
    DoneCause(CauseToken),
}

#[derive(Debug)]
enum SeqData {
    FromList(Value),
    Generator {
        execution: usize,
        code: memory::CodeId,
        captures: Vec<Value>,
        limit: Option<usize>,
    },
}

#[derive(Debug)]
enum IteratorState {
    List {
        backing: Value,
        next: usize,
        end: usize,
    },
    Generator {
        id: usize,
    },
    Exhausted,
}

#[derive(Debug)]
enum ExitData {
    Success(Value),
    Failure(Value),
}

#[derive(Clone, Debug)]
enum DurationData {
    Millis(f64),
    Nanos(num_bigint::BigInt),
    Infinite,
    NegativeInfinite,
}

#[derive(Clone, Debug)]
enum TimeZoneData {
    Offset(f64),
    Named {
        id: String,
        fixed_seconds: Option<i32>,
        is_local: bool,
    },
}

#[derive(Clone, Debug)]
struct DateTimeData {
    ms: f64,
    zone: Option<TimeZoneData>,
}

#[derive(Clone, Debug)]
struct CronData {
    tz: TimeZoneData,
    fields: [Vec<i32>; 6],
    and: bool,
    first: [i32; 6],
    last: [i32; 6],
    next: [Vec<i32>; 6],
    prev: [Vec<i32>; 6],
}

#[derive(Debug)]
struct GraphNode {
    index: i32,
    data: Value,
}

#[derive(Debug)]
struct GraphEdge {
    index: i32,
    source: i32,
    target: i32,
    data: Value,
}

#[derive(Debug)]
struct GraphData {
    directed: bool,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    adj: BTreeMap<i32, Vec<i32>>,
    radj: BTreeMap<i32, Vec<i32>>,
    next_node: i32,
    next_edge: i32,
}

#[derive(Debug)]
struct GraphWalkerData {
    indices: Vec<Value>,
    values: Vec<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReuseKind {
    Tagged,
    Record,
}

#[derive(Debug)]
enum Data {
    /// Detached 0-child slot held by a frame-limited reuse token between
    /// `DropReuse` and a matching `Reuse`/`ReuseWithDicts`/`ReuseRecord` or `Drop`.
    ReuseToken {
        kind: ReuseKind,
        capacity: usize,
    },
    RecoveryResult {
        execution: usize,
        recovery_id: usize,
        selected: Option<Value>,
    },
    Cause(causes::CauseData),
    CauseReason(causes::StoredReason),
    Duration(DurationData),
    Defect(DefectSpec),
    ServiceTag {
        key: String,
    },
    Layer(LayerData),
    String(String),
    /// Exact UTF-16 code units for strings containing lone surrogates.
    Utf16(Vec<u16>),
    /// Immutable packed byte buffer (`packages/std/src/Bytes.fl`).
    Bytes(Vec<u8>),
    /// JSON `null` leaf value shared by `Json`, `JsonPatch`, `Schema`, and `Codec`.
    JsonNull,
    /// Exact decimal `value * 10^-scale` (`packages/std/src/BigDecimal.fl`).
    BigDecimal {
        value: num_bigint::BigInt,
        scale: i64,
    },
    /// Compiled immutable regular expression (`packages/std/src/RegExp.fl`).
    RegExp {
        pattern: String,
        flags: String,
        global: bool,
        compiled: Regex,
    },
    BigInt(String),
    Tagged {
        tag: String,
        /// Physical storage owns ordinary prefix AND evidence suffix.
        ordinary_fields: usize,
        fields: Vec<Value>,
    },
    /// O(1) counted view over a flat `$list` `Data::Tagged` backing array.
    ListSlice {
        backing: Value,
        offset: usize,
        len: usize,
    },
    Record(Vec<(String, Value)>),
    Dictionary(Vec<(String, DictionaryValue)>),
    Closure {
        execution: usize,
        code: memory::CodeId,
        captures: Vec<Value>,
    },
    Task {
        execution: usize,
        code: memory::CodeId,
        captures: Vec<Value>,
    },
    Continuation {
        execution: usize,
        id: usize,
    },
    Cell {
        execution: usize,
        contents: Option<Value>,
    },
    Var {
        execution: usize,
        contents: Option<Value>,
    },
    Lazy {
        execution: usize,
        /// `Some(fiber)` for an O3 local cell (`LazyMakeLocal`), `None` for shared `LazyMake`.
        local_owner: Option<usize>,
        state: Option<LazyState>,
    },
    Arena {
        execution: usize,
        contents: Vec<Value>,
    },
    Seq(SeqData),
    Iterator {
        execution: usize,
        state: IteratorState,
    },
    Label(String),
    Exit(ExitData),
    DateTime(DateTimeData),
    TimeZone(TimeZoneData),
    Cron(Box<CronData>),
    Graph(Box<GraphData>),
    GraphWalker(GraphWalkerData),
    /// Immutable built service context (`Context r`).
    Context(BTreeMap<String, Value>),
    /// Fiber-local reference definition (`Context.Reference a`).
    ContextRef {
        key: String,
        default_fn: Value,
    },
    /// Execution-scoped handle to a cooperative fiber (`Fiber e a`).
    FiberHandle {
        execution: usize,
        id: usize,
    },
    /// Execution-scoped handle to a one-shot `Deferred e a`.
    Deferred {
        execution: usize,
        id: usize,
    },
    /// Execution-scoped handle to a `Layer.MemoMap`.
    MemoMap {
        execution: usize,
        id: usize,
    },
    /// Execution-scoped handle to a `FiberSet` / `FiberMap` / `FiberHandle`.
    FiberGroup {
        execution: usize,
        id: usize,
    },
    /// Callable `restore : Eff -> Eff` passed to `uninterruptible_mask`/`interruptible_mask`.
    MaskRestore {
        execution: usize,
        outer_mask: usize,
    },
    /// Task returned by `MaskRestore(task)` running at `target_mask`.
    MaskedTask {
        target_mask: usize,
        task: Value,
    },
    /// Task returned by `Effect.cached*` that reads or runs `CachedCell(id)`.
    CachedRunTask {
        execution: usize,
        id: usize,
    },
    /// Task returned by `Effect.cached_invalidate_with_ttl` clearing `CachedCell(id)`.
    CachedInvalidateTask {
        execution: usize,
        id: usize,
    },
    /// Task returned by `Deferred.poll` yielding or failing with `ExitData`.
    ExitTask(ExitData),
    /// Captured services and fiber-local refs for running acquired resources later.
    CapturedEnv {
        services: Vec<Value>,
        refs: BTreeMap<String, Value>,
    },
    /// Counted handle to a host object/function/Effect owned by the Node `HostBridge`.
    ForeignHandle {
        execution: usize,
        host_id: u64,
    },
    /// Counted handle marking a cycle back-edge in an untrusted host value.
    ForeignCycle {
        execution: usize,
        host_id: u64,
    },
}

#[derive(Debug)]
struct Object {
    references: usize,
    data: Data,
}

#[derive(Debug)]
struct Slot {
    generation: usize,
    object: Option<Object>,
}

#[derive(Debug)]
pub struct Heap {
    identity: usize,
    objects: Vec<Slot>,
    free_list: Vec<usize>,
    stats: Stats,
    reuse_enabled: bool,
    // Weak metadata: only live tag identities, never owned language tokens.
    service_tags: BTreeMap<String, ObjectId>,
    // Freed unresumed continuations queued for deterministic VM unwind/finalization.
    dropped_continuations: Vec<(usize, usize)>,
    // Freed foreign handles queued for deterministic release on the Node HostBridge.
    released_foreign_handles: Vec<(usize, u64)>,
}

static NEXT_HEAP: AtomicUsize = AtomicUsize::new(0);

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}
