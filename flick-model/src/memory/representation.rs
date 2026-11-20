#[derive(Debug)]
enum LayerData {
    Succeed { tag: Value, implementation: Value },
    // Strong graph edges: duplicate roots retain their independent tokens.
    // Dynamic acquisition constructors are deliberately not represented yet.
    Merge { left: Value, right: Value },
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
    /// Retains the original task until normal or typed-error completion so a
    /// defect/abort/signal restores Pending cleanly (`#626`).
    Running(Value),
    DoneOk(Value),
    DoneErr(Value),
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

#[derive(Debug)]
enum Data {
    RecoveryResult {
        execution: usize,
        recovery_id: usize,
        selected: Option<Value>,
    },
    Cause(Vec<causes::StoredReason>),
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
    // Weak metadata: only live tag identities, never owned language tokens.
    service_tags: BTreeMap<String, ObjectId>,
    // Freed unresumed continuations queued for deterministic VM unwind/finalization.
    dropped_continuations: Vec<(usize, usize)>,
}

static NEXT_HEAP: AtomicUsize = AtomicUsize::new(0);

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}
