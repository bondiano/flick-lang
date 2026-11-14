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
    Cell {
        execution: usize,
        contents: Option<Value>,
    },
    Arena {
        execution: usize,
        contents: Vec<Value>,
    },
}

#[derive(Debug)]
struct Object {
    references: usize,
    data: Data,
}

#[derive(Debug)]
pub struct Heap {
    identity: usize,
    objects: Vec<Option<Object>>,
    stats: Stats,
    // Weak metadata: only live tag identities, never owned language tokens.
    service_tags: BTreeMap<String, ObjectId>,
}

static NEXT_HEAP: AtomicUsize = AtomicUsize::new(0);

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}
