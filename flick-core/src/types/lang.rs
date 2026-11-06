//! Lang items: the built-in entities the compiler itself relies on. A
//! built-in module marks one with `@lang name` before its declaration (or
//! before a constructor), and the compiler asks for it by [`Lang`] instead
//! of spelling its module and name, so the library may move or rename it.

#[cfg(test)]
use std::cell::Cell;
use std::sync::OnceLock;

use crate::iface::{Entity, Namespace};
use crate::source::SourceMap;
use crate::syntax::{self, cst::Node, kind::SyntaxKind as K};

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
    // the module of the built-in instances
    Instances = "instances",
    // class methods that syntax stands for
    Map2 = "map2",
    FlatMap = "flat_map",
    Nothing = "nothing",
    Iter = "iter",
    ShowMethod = "show_method",
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

/// Where a lang item is declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub module: String,
    pub namespace: Namespace,
    pub name: String,
    /// `module.name`, as a service row spells it.
    pub label: String,
}

impl Item {
    /// Whether the item is `module.name`.
    pub fn is(&self, module: &str, name: &str) -> bool {
        self.module == module && self.name == name
    }

    pub fn entity(&self) -> Entity {
        Entity {
            module: self.module.clone(),
            namespace: self.namespace,
            name: self.name.clone(),
        }
    }
}

/// The lang items of a set of modules, indexed by [`Lang`].
#[derive(Debug)]
pub struct Table {
    items: Vec<Option<Item>>,
}

impl Table {
    /// The lang items the modules `sources` declare, as `(module, text)`.
    pub fn of(sources: &[(&str, &str)]) -> Table {
        let mut table = Table {
            items: vec![None; Lang::ALL.len()],
        };
        for &(module, text) in sources {
            let mut map = SourceMap::default();
            let file = map.add(format!("<builtin>/{module}.fl"), text);
            let parse = syntax::parse(text, file);
            for (lang, namespace, name) in declared(&parse.tree, text) {
                let name = name.unwrap_or_else(|| module.to_owned());
                let item = Item {
                    label: format!("{module}.{name}"),
                    module: module.to_owned(),
                    namespace,
                    name,
                };
                if let Some(old) = table.items[lang as usize].replace(item) {
                    panic!(
                        "lang item `{}` is declared twice, in {} and {module}",
                        lang.name(),
                        old.module
                    );
                }
            }
        }
        table
    }

    /// The declaration of `lang`, if the modules declare it.
    pub fn find(&self, lang: Lang) -> Option<&Item> {
        self.items[lang as usize].as_ref()
    }

    pub fn get(&self, lang: Lang) -> &Item {
        self.find(lang)
            .unwrap_or_else(|| panic!("no built-in module declares `@lang {}`", lang.name()))
    }
}

/// The lang items of the built-in modules.
pub fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    #[cfg(test)]
    if let Some(table) = OVERRIDE.with(Cell::get) {
        return table;
    }
    TABLE.get_or_init(|| Table::of(super::builtin_sources()))
}

#[cfg(test)]
thread_local! {
    static OVERRIDE: Cell<Option<&'static Table>> = const { Cell::new(None) };
}

/// Runs `f` with the lang items of `table` instead of the built-in ones, on
/// this thread.
#[cfg(test)]
pub fn with_table<R>(table: Table, f: impl FnOnce() -> R) -> R {
    let table: &'static Table = Box::leak(Box::new(table));
    let old = OVERRIDE.with(|o| o.replace(Some(table)));
    let out = f();
    OVERRIDE.with(|o| o.set(old));
    out
}

/// The declaration of `lang` among the built-in modules.
pub fn item(lang: Lang) -> &'static Item {
    table().get(lang)
}

/// The module that declares `lang`, if a built-in module does.
pub fn module_of(lang: Lang) -> Option<&'static str> {
    table().find(lang).map(|item| item.module.as_str())
}

/// `module.name` of `lang`, as a service row spells it, if the modules
/// declare it.
pub fn label(lang: Lang) -> Option<&'static str> {
    table().find(lang).map(|item| item.label.as_str())
}

/// The module whose names every module sees unqualified.
pub fn prelude() -> &'static str {
    &item(Lang::Prelude).module
}

/// Whether `module.name` is the lang item `lang`; nothing is an item the
/// built-in modules leave out.
pub fn is(lang: Lang, module: &str, name: &str) -> bool {
    table().find(lang).is_some_and(|item| item.is(module, name))
}

/// The `@lang` attributes of a module with the names they mark: `None` for
/// the module itself, an attribute before its header.
pub fn declared(tree: &Node, text: &str) -> Vec<(Lang, Namespace, Option<String>)> {
    let mut found = Vec::new();
    collect(tree, text, &mut found);
    found
}

fn collect(node: &Node, text: &str, found: &mut Vec<(Lang, Namespace, Option<String>)>) {
    let mut pending: Option<Lang> = None;
    for child in node.nodes() {
        match child.kind {
            // `@derive` beside `@lang` marks the same declaration
            K::Attribute => pending = lang_of(child, text).or(pending),
            K::Members => collect(child, text, found),
            K::TypeDecl => {
                for variant in child.nodes().filter(|n| n.kind == K::Variant) {
                    let marked = variant.nodes().find(|n| n.kind == K::Attribute);
                    if let Some(lang) = marked.and_then(|a| lang_of(a, text)) {
                        let name = first(variant, K::Con, text);
                        found.push((lang, Namespace::Constructor, name));
                    }
                }
            }
            _ => {}
        }
        if child.kind == K::Attribute {
            continue;
        }
        if let Some(lang) = pending.take() {
            found.push(marked(lang, child, text));
        }
        if matches!(child.kind, K::ClassDecl | K::ServiceDecl) {
            collect(child, text, found);
        }
    }
}

/// The lang item `lang` marks on `decl`.
fn marked(lang: Lang, decl: &Node, text: &str) -> (Lang, Namespace, Option<String>) {
    match decl.kind {
        K::ModuleHeader => (lang, Namespace::Type, None),
        K::TypeDecl | K::ErrorDecl | K::ClassDecl | K::ServiceDecl => {
            (lang, Namespace::Type, first(decl, K::Con, text))
        }
        _ => (lang, Namespace::Value, first(decl, K::Ident, text)),
    }
}

/// The text of the first token of `kind` right under `node`.
fn first(node: &Node, kind: K, text: &str) -> Option<String> {
    node.child_tokens()
        .find(|t| t.kind == kind)
        .map(|t| t.text(text).to_owned())
}

/// The attributes of `tree`, at any depth.
pub fn attributes(tree: &Node) -> Vec<&Node> {
    let mut found = Vec::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        for child in node.nodes() {
            if child.kind == K::Attribute {
                found.push(child);
            } else {
                stack.push(child);
            }
        }
    }
    found
}

/// The lang item an attribute names, if it is `@lang item`.
pub fn lang_of(attribute: &Node, text: &str) -> Option<Lang> {
    let words: Vec<&str> = attribute
        .child_tokens()
        .filter(|t| t.kind == K::Ident)
        .map(|t| t.text(text))
        .collect();
    match words[..] {
        ["lang", item] => Lang::from_name(item),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, table};

    /// Each lang item names one declaration of the built-in modules.
    #[test]
    fn every_lang_item_is_declared() {
        let table = table();
        for &lang in Lang::ALL {
            let item = table.get(lang);
            assert!(!item.name.is_empty(), "{lang:?}");
            assert_eq!(Lang::from_name(lang.name()), Some(lang));
        }
        assert_eq!(table.get(Lang::Some).name, "Some");
        assert_eq!(table.get(Lang::FlatMap).module, "Prelude");
        assert_eq!(table.get(Lang::Instances).name, "Instances");
    }
}
