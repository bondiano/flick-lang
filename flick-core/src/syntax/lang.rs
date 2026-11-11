//! The names of lang items: what `@lang name` may say. What each stands
//! for is the checker's to know.

macro_rules! lang_items {
    ($($item:ident = $name:literal,)*) => {
        /// A lang item, spelled in snake case after `@lang`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Lang {
            $($item,)*
        }

        impl Lang {
            pub const ALL: &[Lang] = &[$(Lang::$item,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(Lang::$item => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<Lang> {
                match name {
                    $($name => Some(Lang::$item),)*
                    _ => None,
                }
            }
        }
    };
}

lang_items! {
    // the module whose names every module sees unqualified
    Prelude = "prelude",
    // types
    Unit = "unit",
    Never = "never",
    Bool = "bool",
    Int = "int",
    Float = "float",
    BigInt = "bigint",
    String = "string",
    Unknown = "unknown",
    JsValue = "js_value",
    List = "list",
    Seq = "seq",
    Ref = "ref",
    Redacted = "redacted",
    Schema = "schema",
    Url = "url",
    Form = "form",
    PureCodec = "pure_codec",
    Eff = "eff",
    Layer = "layer",
    Codec = "codec",
    Errors = "errors",
    Ui = "ui",
    ServiceTag = "service_tag",
    Scope = "scope",
    // what may suspend, which the runtime's scheduler handles at the root
    Async = "async",
    Setup = "setup",
    AtomRead = "atom_read",
    // what `ui` and atoms are made of, which their checks name
    Atom = "atom",
    AtomWritable = "atom_writable",
    AtomBrowser = "atom_browser",
    AtomFamily = "atom_family",
    ReactivityAtom = "reactivity_atom",
    ReactivityWritable = "reactivity_writable",
    AtomWrite = "atom_write",
    AtomRegistry = "atom_registry",
    UiView = "ui_view",
    UiHandler = "ui_handler",
    UiComponent = "ui_component",
    Action = "action",
    View = "view",
    EventExhaust = "event_exhaust",
    EventLatest = "event_latest",
    EventQueue = "event_queue",
    EventConcurrent = "event_concurrent",
    ActionQueue = "action_queue",
    ActionConcurrent = "action_concurrent",
    Serializable = "serializable",
    SerializableFamily = "serializable_family",
    SerializableTask = "serializable_task",
    SerializableResult = "serializable_result",
    ServiceImpl = "service_impl",
    Option = "option",
    Result = "result",
    Exit = "exit",
    Failure = "failure",
    Json = "json",
    SchemaError = "schema_error",
    AppError = "app_error",
    // constructors
    None = "none",
    Some = "some",
    Err = "err",
    Ok = "ok",
    // classes
    Eq = "eq",
    Ord = "ord",
    Hash = "hash",
    Show = "show",
    Iterable = "iterable",
    Stream = "stream",
    Numeric = "numeric",
    Integer = "integer",
    Floating = "floating",
    // the classes the compiler solves for records, and their types
    Label = "label",
    Field = "field",
    HasField = "has_field",
    Lacks = "lacks",
    Remove = "remove",
    Project = "project",
    UnionDisjoint = "union_disjoint",
    MapFields = "map_fields",
    Labels = "labels",
    PathParams = "path_params",
    // what scopes and fibers own
    Owner = "owner",
    Outlives = "outlives",
    Provided = "provided",
    Detached = "detached",
    // the module of the built-in instances
    Instances = "instances",
    // class methods that syntax stands for
    Map2 = "map2",
    FlatMap = "flat_map",
    Nothing = "nothing",
    Iter = "iter",
    ShowMethod = "show_method",
    DisplayMethod = "display_method",
    Add = "add",
    Sub = "sub",
    Mul = "mul",
    Div = "div",
    Rem = "rem",
    Pow = "pow",
    Negate = "negate",
    FromInt = "from_int",
    FromFloat = "from_float",
    Lt = "lt",
    Le = "le",
    Gt = "gt",
    Ge = "ge",
    // functions that syntax stands for or the optimizer knows
    FromResult = "from_result",
    Fail = "fail",
    LayerRun = "layer_run",
    ScopeRun = "scope_run",
    ScopeCurrent = "scope_current",
    Pure = "pure",
    OptionBind = "option_bind",
    ResultBind = "result_bind",
    OrdLt = "ord_lt",
    OrdLe = "ord_le",
    OrdGt = "ord_gt",
    OrdGe = "ord_ge",
    SeqFrom = "seq_from",
    SeqRange = "seq_range",
    SeqRangeTo = "seq_range_to",
    SeqMap = "seq_map",
    SeqFilter = "seq_filter",
    SeqFlatMap = "seq_flat_map",
    SeqTake = "seq_take",
    SeqToList = "seq_to_list",
    ListMap = "list_map",
    ListFilter = "list_filter",
    RecordLabel = "record_label",
    WireCodec = "wire_codec",
    CodecJson = "codec_json",
    // what a server fn needs and serves as, which E009 and E011 name
    Server = "server",
    ServerFunction = "server_function",
    ServerGuarded = "server_guarded",
    // the live page, which a view's render does not reach
    UiDocument = "ui_document",
    FloatInfinity = "float_infinity",
    FloatNan = "float_nan",
    LayerMerge = "layer_merge",
    // the class the compiler gives every enum an instance of
    Enum = "enum",
    // the evidence of two types being one, which a GADT branch assumes
    TypeEq = "type_eq",
    CallSite = "call_site",
    // the class `deriving Generic` gives an instance of, and its representation
    Generic = "generic",
    GenericSum = "generic_sum",
    GenericCon = "generic_con",
    GenericField = "generic_field",
    TodoAt = "todo_at",
    EchoAt = "echo_at",
    AssertAt = "assert_at",
}
