//! The IR itself: modules of items, expressions with every application
//! stage, dictionary and lift explicit, patterns and literals.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    Arith, Constraint, Constructor, Dict, Foreign, Handler, InstanceInfo, Lift, Namespace, Num,
    Span, Stage, TypeDecl, TypeRef,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    /// Top-level definitions in source order; code generation orders them
    /// by what each one uses.
    pub items: Vec<Item>,
    /// Data, error and nominal type declarations of this module.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<TypeDecl>,
    /// The top-level values debugging tools know by `Module.name`.
    pub labels: BTreeMap<String, Label>,
    #[serde(default, skip_serializing_if = "Initialization::is_empty")]
    pub initialization: Initialization,
    /// The constructors whose one field the declaration writes as a closed
    /// record, each with the labels of that record in the order written: a
    /// backend may hold such a record in the value of the constructor.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub records: BTreeMap<String, Vec<String>>,
    /// The package the module belongs to, which names its constructors in
    /// their tags; `None` for a built-in module and one of no package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// What the module exports, as its interface says.
    #[serde(default, skip_serializing_if = "Exports::is_empty")]
    pub exports: Exports,
}

/// The exports of a module: its own values, which other modules and the
/// host call, and what it exports of other modules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exports {
    /// The exported values of the module itself.
    pub values: Vec<Exported>,
    /// The entities of other modules the module exports.
    pub reexports: Vec<Reexport>,
}

impl Exports {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.reexports.is_empty()
    }
}

/// An exported value of a module itself: its name, and the arrows a call
/// supplies when its last stage is Sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exported {
    pub name: String,
    pub sync: Option<usize>,
}

/// An entity of another module that a module exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reexport {
    pub module: String,
    pub namespace: Namespace,
    pub name: String,
}

/// Lexical source prefixes keep canonical entity owners while the backends
/// schedule their initializers independently of the rest of the module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Initialization {
    pub prefixes: Vec<Prefix>,
}

impl Initialization {
    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefix {
    pub before: u32,
    pub bindings: Vec<String>,
}

/// The label of a top-level value, `Module.name`, which debugging tools
/// show: of a kind its checker names, which the backend and `[build]
/// labels` read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

impl Item {
    /// Each expression the item holds: a body, the methods of an instance
    /// and the `catch` mapping of a foreign function.
    pub fn each_body<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        match &self.kind {
            ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => f(body),
            ItemKind::Instance { methods, .. } => methods.iter().for_each(|(_, m)| f(m)),
            ItemKind::Foreign(foreign) => foreign.catch.iter().for_each(f),
            ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    /// `let name … = …`; a definition with class constraints takes their
    /// dictionaries first.
    Value {
        name: String,
        /// Quantified layout-relevant type parameters of this definition.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_params: Vec<String>,
        dicts: Vec<String>,
        /// Class constraints matching `dicts` in order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        dict_constraints: Vec<Constraint>,
        /// The layout-relevant type of the definition in terms of `type_params`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ty: Option<TypeRef>,
        body: Expr,
    },
    /// `let (a, b) = …`.
    Destructure {
        pattern: Pat,
        /// Types of the pattern's bound variables, in `Pat::binders` order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        binder_types: Vec<(String, TypeRef)>,
        body: Expr,
    },
    /// A function of the host, called by its convention.
    Foreign(Foreign),
    /// A constructor of a type or error declared here.
    Constructor(Constructor),
    /// `service S`: its key is `Module.S`.
    Service { name: String, key: String },
    /// An instance with its methods; with a context, a function of the
    /// context's dictionaries.
    Instance {
        info: Box<InstanceInfo>,
        methods: Vec<(String, Expr)>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// The checker's layout-relevant type of the expression, when available.
    pub ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Lit(Lit),
    /// A local by its unique name in the enclosing top-level definition.
    Local(String),
    /// What the `Stmt::Var` of this name holds where this is evaluated: the
    /// value of its last `Stmt::Assign`. Unlike a `Local`, two reads may
    /// differ, so no pass copies or moves one.
    Var(String),
    /// A top-level value of a module.
    Global {
        module: String,
        name: String,
        /// Type arguments instantiating the global's quantified type parameters.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<TypeRef>,
    },
    /// The method `name` of a class dictionary.
    Method {
        dict: Dict,
        name: String,
        /// Type arguments instantiating the method's quantified type parameters.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<TypeRef>,
    },
    /// A constructor as a function of its fields (a value without fields).
    Con(Constructor),
    /// `E.op`, looked up at its first runtime stage.
    Perform {
        effect: String,
        op: String,
        stages: Vec<Stage>,
    },
    /// A service named as a value.
    ServiceTag(String),
    /// `Record.label` at a known label.
    Label(String),
    /// The elements `body` yields: a lazy `Seq` whose generator runs `body`
    /// on each iteration, or with `strict` a `List` built at once.
    Generate {
        body: Box<Expr>,
        strict: bool,
        /// A lazy one whose body makes Sync calls: each step reads the
        /// services of the stage that built it.
        sync: bool,
        /// A lazy one that ends after this many elements (`Seq.take` fused),
        /// evaluated when it is built.
        limit: Option<Box<Expr>>,
    },
    /// Runs `body` for each element of `source` that `pat` matches; only
    /// inside a `Generate`.
    Each {
        /// Stop before requesting the next element, for an imperative `for`.
        #[serde(default)]
        stop: Option<Box<Expr>>,
        pat: Pat,
        /// Types of the pattern's bound variables, in `Pat::binders` order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        binder_types: Vec<(String, TypeRef)>,
        source: Box<Expr>,
        body: Box<Expr>,
    },
    /// An element of the enclosing `Generate`.
    Yield(Box<Expr>),
    /// A sequential loop. The condition is reevaluated on every iteration.
    Loop {
        condition: Box<Expr>,
        body: Box<Expr>,
    },
    /// Source loop and its lexical identity; exits may cross scope callbacks.
    ControlLoop {
        id: String,
        condition: Option<Box<Expr>>,
        body: Box<Expr>,
        iterable: bool,
    },
    /// Some(value) breaks, None continues the named source loop.
    LoopExit {
        id: String,
        value: Option<Box<Expr>>,
    },
    /// A constrained value given its dictionaries.
    Dicts(Box<Expr>, Vec<Dict>),
    /// A value checked against a constrained type: takes the dictionaries.
    Abstract(Vec<String>, Box<Expr>),
    /// One stage: `func` applied to `arg`, a call of `stage`. With
    /// `arg_first` (`arg |> func`) the argument is evaluated first.
    App {
        func: Box<Expr>,
        arg: Box<Expr>,
        stage: Stage,
        arg_first: bool,
    },
    /// A function of one or more stages; the body runs at the last one.
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
    },
    Block(Vec<Stmt>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<Arm>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    /// Fields in the order written, which is the order of evaluation.
    Record(Vec<(String, Expr)>),
    /// A class dictionary assembled from explicit methods and superclasses.
    Dictionary(Vec<(String, Expr)>),
    Update(Box<Expr>, Vec<(String, Expr)>),
    Field(Box<Expr>, String),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    /// `o?`: the value of the `Some`; a `None` returns from the function.
    Unwrap(Box<Expr>),
    /// `-e` on a number of the type.
    Neg(Num, Box<Expr>),
    Ignore(Box<Expr>),
    Await(Box<Expr>),
    Task(Box<Expr>),
    /// A runtime call that acquires into the owner bound to the local, not
    /// into the innermost scope.
    Within(String, Box<Expr>),
    /// Reads inert lexical owner evidence at execution, not construction.
    /// This contextual read cannot be hoisted across an invocation or task.
    ReadOwnerEvidence,
    /// Executes an immediate non-pure application or await with new lexical
    /// evidence, without changing the default work owner. Operands of the
    /// invocation are immutable locals/literals evaluated before this node.
    /// The first expression is evaluated under the previous evidence; Unit
    /// denotes absence, not a root owner. Every exit restores the old evidence.
    WithOwnerEvidence(Box<Expr>, Box<Expr>),
    /// The handler of effects of `try` arms for operations or `provide S impl`.
    Handle(Box<Handler>),
    /// A pure function value used where a runtime one is expected.
    Lift(Box<Expr>, Lift),
}

/// How many arguments `S.op` of `stages` takes before it looks its service
/// up: those up to its first runtime stage, or all of them.
pub fn operation_arity(stages: &[Stage]) -> usize {
    stages
        .iter()
        .position(|&s| s != Stage::Pure)
        .map_or(stages.len(), |i| i + 1)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub pat: Pat,
    /// The stage of the arrow of this parameter.
    pub stage: Stage,
    /// The parameter's layout-relevant type, when available.
    pub ty: Option<TypeRef>,
    /// Types of pattern variables bound by `pat`, in `Pat::binders` order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binder_types: Vec<(String, TypeRef)>,
}

impl Param {
    /// Type of a variable `name` bound by this parameter's pattern.
    pub fn binder_type(&self, name: &str) -> Option<&TypeRef> {
        self.binder_types
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, ty)| ty)
            .or_else(|| {
                matches!(&self.pat, Pat::Bind(n) if n == name)
                    .then_some(self.ty.as_ref())
                    .flatten()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    Let {
        pat: Pat,
        /// Types of pattern variables bound by `pat`, in `Pat::binders` order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        binder_types: Vec<(String, TypeRef)>,
        value: Expr,
    },
    /// `let mut name = value`: a variable of the enclosing function, read
    /// as a `Var` and changed by an `Assign` after it. No function inside
    /// the function uses it, but the body of `Scope.run` or a layer's run,
    /// which runs once, in place.
    Var {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ty: Option<TypeRef>,
        value: Expr,
    },
    /// `name = value` of a `Var` in scope.
    Assign(String, Expr),
    /// A local function; `rec` when it may call itself.
    Fun {
        name: String,
        rec: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_params: Vec<String>,
        dicts: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        dict_constraints: Vec<Constraint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ty: Option<TypeRef>,
        value: Expr,
    },
    Expr(Expr),
}

impl Stmt {
    pub fn let_bind(pat: Pat, value: Expr) -> Stmt {
        Stmt::Let {
            pat,
            binder_types: Vec::new(),
            value,
        }
    }

    pub fn let_typed(pat: Pat, binder_types: Vec<(String, TypeRef)>, value: Expr) -> Stmt {
        Stmt::Let {
            pat,
            binder_types,
            value,
        }
    }

    pub fn var(name: String, ty: Option<TypeRef>, value: Expr) -> Stmt {
        Stmt::Var { name, ty, value }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arm {
    pub pat: Pat,
    /// Types of pattern variables bound by `pat`, in `Pat::binders` order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binder_types: Vec<(String, TypeRef)>,
    pub guard: Option<Expr>,
    pub body: Expr,
}

/// A literal; a `Float` or `Float32` equals another with the same bits, and
/// any NaN another NaN: `0.0` is not `-0.0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Lit {
    Int(i32),
    Int8(i8),
    Int16(i16),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float(#[serde(with = "crate::float")] f64),
    Float32(#[serde(with = "crate::float::f32")] f32),
    /// Decimal or `0x`/`0o`/`0b` digits with an optional `-`, without `_`
    /// or a suffix: its size is not bounded.
    BigInt(String),
    String(String),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pat {
    Wild,
    Bind(String),
    /// A literal; `Float` and `BigInt` may be negative.
    Lit(Lit),
    Tuple(Vec<Pat>),
    /// Items, then the pattern of the rest of a `[a, ...rest]`.
    List(Vec<Pat>, Option<Box<Pat>>),
    /// Fields a record pattern mentions.
    Record(Vec<(String, Pat)>),
    Con(Constructor, Vec<Pat>),
    Or(Vec<Pat>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Or,
    And,
    /// Structural equality; `SameValueZero` on `Float`.
    Eq,
    NotEq,
    /// `===`: equality of values whose identity is their equality (`String`,
    /// `Bool`, `BigInt`, the tags of constructors without fields).
    Same,
    NotSame,
    Lt,
    Le,
    Gt,
    Ge,
    /// `++` on strings.
    Concat,
    /// `++` on lists.
    ListConcat,
    /// Arithmetic on the numeric type.
    Arith(Arith, Num),
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Or => "||",
            BinOp::And => "&&",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Same => "===",
            BinOp::NotSame => "!==",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Concat => "++",
            BinOp::ListConcat => "++list",
            BinOp::Arith(op, _) => op.symbol(),
        }
    }
}

impl Expr {
    // Share initialization of the large IR node in the browser compiler.
    #[cfg_attr(target_arch = "wasm32", inline(never))]
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr {
            kind,
            span,
            ty: None,
        }
    }

    pub fn with_ty(kind: ExprKind, span: Span, ty: Option<TypeRef>) -> Self {
        Expr { kind, span, ty }
    }
}

impl Pat {
    /// Names the pattern binds, in order.
    pub fn binders<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Pat::Bind(name) => out.push(name),
            Pat::Wild | Pat::Lit(_) => {}
            Pat::Tuple(items) | Pat::Con(_, items) => {
                for p in items {
                    p.binders(out);
                }
            }
            Pat::List(items, rest) => {
                for p in items.iter().chain(rest.as_deref()) {
                    p.binders(out);
                }
            }
            Pat::Record(fields) => {
                for (_, p) in fields {
                    p.binders(out);
                }
            }
            // every alternative binds the same names
            Pat::Or(alternatives) => {
                if let Some(first) = alternatives.first() {
                    first.binders(out);
                }
            }
        }
    }

    /// Whether every value matches.
    pub fn irrefutable(&self) -> bool {
        match self {
            Pat::Wild | Pat::Bind(_) | Pat::Lit(Lit::Unit) => true,
            Pat::Tuple(items) => items.iter().all(Pat::irrefutable),
            Pat::Record(fields) => fields.iter().all(|(_, p)| p.irrefutable()),
            Pat::Or(alternatives) => alternatives.iter().any(Pat::irrefutable),
            Pat::List(items, rest) => items.is_empty() && rest.is_some(),
            Pat::Lit(_) | Pat::Con(..) => false,
        }
    }
}

impl PartialEq for Lit {
    fn eq(&self, other: &Lit) -> bool {
        match (self, other) {
            (Lit::Int(a), Lit::Int(b)) => a == b,
            (Lit::Int8(a), Lit::Int8(b)) => a == b,
            (Lit::Int16(a), Lit::Int16(b)) => a == b,
            (Lit::Int64(a), Lit::Int64(b)) => a == b,
            (Lit::UInt8(a), Lit::UInt8(b)) => a == b,
            (Lit::UInt16(a), Lit::UInt16(b)) => a == b,
            (Lit::UInt32(a), Lit::UInt32(b)) => a == b,
            (Lit::UInt64(a), Lit::UInt64(b)) => a == b,
            (Lit::Float(a), Lit::Float(b)) => {
                a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
            }
            (Lit::Float32(a), Lit::Float32(b)) => {
                a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
            }
            (Lit::BigInt(a), Lit::BigInt(b)) | (Lit::String(a), Lit::String(b)) => a == b,
            (Lit::Bool(a), Lit::Bool(b)) => a == b,
            (Lit::Unit, Lit::Unit) => true,
            _ => false,
        }
    }
}
