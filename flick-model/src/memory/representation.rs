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

#[derive(Debug)]
enum Data {
    RecoveryResult {
        execution: usize,
        recovery_id: usize,
        selected: Option<Value>,
    },
    Cause(Vec<causes::StoredReason>),
    Defect(DefectSpec),
    ServiceTag {
        key: String,
    },
    Layer(LayerData),
    String(String),
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
