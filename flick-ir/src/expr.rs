//! The IR itself: modules of items, expressions with every application
//! stage, dictionary and lift explicit, patterns and literals.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    Arith, Constructor, Dict, Foreign, Handler, InstanceInfo, Lift, Num, Span, Stage, TypeName,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    /// Top-level definitions in source order; code generation orders them
    /// by what each one uses.
    pub items: Vec<Item>,
    /// The top-level values debugging tools know by `Module.name`.
    pub labels: BTreeMap<String, Label>,
    #[serde(default, skip_serializing_if = "Initialization::is_empty")]
    pub initialization: Initialization,
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
    /// Each expression the item holds: a body, the check of a refined type,
    /// the methods of an instance and the `catch` mapping of a foreign
    /// function.
    pub fn each_body<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        match &self.kind {
            ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => f(body),
            ItemKind::Refined { check, .. } => f(check),
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
        dicts: Vec<String>,
        body: Expr,
    },
    /// `let (a, b) = …`.
    Destructure { pattern: Pat, body: Expr },
    /// A function of the host, called by its convention.
    Foreign(Foreign),
    /// A constructor of a type or error declared here.
    Constructor(Constructor),
    /// `service S`: its key is `Module.S`.
    Service { name: String, key: String },
    /// A refined type `name`: its check `T$check`, and `T$parse`, `T$value`.
    Refined { name: String, check: Expr },
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
    /// The checker's type of the expression when that is a type constructor
    /// on its own (`Float`, `String`); none otherwise.
    pub ty: Option<Arc<TypeName>>,
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
    },
    /// The method `name` of a class dictionary.
    Method {
        dict: Dict,
        name: String,
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
    /// The parameter's type when it is a type constructor on its own, as
    /// [`Expr::ty`].
    pub ty: Option<Arc<TypeName>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    Let(Pat, Expr),
    /// `let mut name = value`: a variable of the enclosing function, read
    /// as a `Local` and changed by an `Assign` after it. No function inside
    /// the function uses it, but the body of `Scope.run` or a layer's run,
    /// which runs once, in place.
    Var(String, Expr),
    /// `name = value` of a `Var` in scope.
    Assign(String, Expr),
    /// A local function; `rec` when it may call itself.
    Fun {
        name: String,
        rec: bool,
        dicts: Vec<String>,
        value: Expr,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Expr,
}

/// A literal; a `Float` equals another with the same bits, and any NaN
/// another NaN: `0.0` is not `-0.0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Lit {
    Int(i32),
    Float(#[serde(with = "crate::float")] f64),
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
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr {
            kind,
            span,
            ty: None,
        }
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
            (Lit::Float(a), Lit::Float(b)) => {
                a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
            }
            (Lit::BigInt(a), Lit::BigInt(b)) | (Lit::String(a), Lit::String(b)) => a == b,
            (Lit::Bool(a), Lit::Bool(b)) => a == b,
            (Lit::Unit, Lit::Unit) => true,
            _ => false,
        }
    }
}
