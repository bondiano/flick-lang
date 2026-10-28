//! Typed IR: a checked module with everything code generation must not
//! guess made explicit: each application stage and whether it runs through
//! the runtime, the dictionaries of class constraints, lifts of pure
//! functions into runtime positions, what every name refers to, and the
//! source range of every node. Evaluation order is the order of the nodes:
//! children left to right, each application stage after its argument.
//!
//! Built from the syntax tree and the checker's [`Typing`](crate::types::Typing)
//! by [`lower`]; printed for tests by [`print`].

mod lower;
mod print;

pub use lower::lower;
pub use print::print;

use std::collections::HashMap;

use crate::numeric::{Arith, Num};
use crate::source::Span;
pub use crate::types::Stage;
use crate::types::{Constructor, Dict, ForeignInfo, InstanceInfo, Lift, SchemaItem};

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    /// Top-level definitions in source order; code generation orders them
    /// by what each one uses.
    pub items: Vec<Item>,
    /// The top-level values debugging tools know by `Module.name`.
    pub labels: HashMap<String, Label>,
}

/// What a labelled top-level value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Atom,
    /// A view or an action, labelled in release too by default.
    Component,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
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
    /// A binding to a JS function; only the JS backend emits it.
    Foreign(Foreign),
    /// A constructor of a type or error declared here.
    Constructor(Constructor),
    /// `service S`: its key is `Module.S`.
    Service { name: String, key: String },
    /// A refined type `name`: its check `T$check`, and `T$parse`, `T$value`.
    Refined { name: String, check: Expr },
    /// The witness `T$schema` of a type deriving `Schema`, a function of
    /// `witnesses` witnesses; a refined type also gets `T$json`.
    Schema(SchemaItem),
    /// An instance with its methods; with a context, a function of the
    /// context's dictionaries.
    Instance {
        info: Box<InstanceInfo>,
        methods: Vec<(String, Expr)>,
    },
}

/// A `foreign` declaration: a JS calling convention, not a portable one.
#[derive(Debug, Clone)]
pub struct Foreign {
    pub name: String,
    pub mode: ForeignMode,
    /// `None` for `global`.
    pub module: Option<String>,
    /// The export, or the global path `Date.now`.
    pub export: String,
    /// Each arrow of the signature: whether it is runtime.
    pub stages: Vec<bool>,
    /// Dictionaries the JS function takes before its arguments.
    pub dicts: usize,
    /// `arity n`, else the stages up to the first runtime one.
    pub arity: usize,
    /// The rest of its calling convention.
    pub info: ForeignInfo,
    /// The `catch` mapping, a function of the thrown value.
    pub catch: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignMode {
    /// A plain JS function.
    Pure,
    /// Synchronous JS run by the runtime; a throw is a defect.
    Sync,
    /// Returns a `Promise`; a rejection is a defect.
    Async,
    /// Returns an `Effect`.
    Effect,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Lit(Lit),
    /// A local by its unique name in the enclosing top-level definition.
    Local(String),
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
    /// `S.op`, looked up at its first runtime stage.
    Operation {
        service: String,
        name: String,
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
        pat: Pat,
        source: Box<Expr>,
        body: Box<Expr>,
    },
    /// An element of the enclosing `Generate`.
    Yield(Box<Expr>),
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
    /// `try body with arms`: the arms see the typed failure.
    Try(Box<Expr>, Vec<Arm>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    /// Fields in the order written, which is the order of evaluation.
    Record(Vec<(String, Expr)>),
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
    /// `scope [s] { … }`: binds its owner (the name of a named scope, or
    /// one a `Within` needs) and the owner around it, when used.
    Scope {
        owner: Option<String>,
        outer: Option<String>,
        body: Box<Expr>,
    },
    /// A runtime call that acquires into the owner bound to the local, not
    /// into the innermost scope.
    Within(String, Box<Expr>),
    Provide(Box<Expr>, Box<Expr>),
    /// A pure function value used where a runtime one is expected.
    Lift(Box<Expr>, Lift),
    /// A `codec` block: the name it decodes, and each label with its field.
    CodecBlock(String, Vec<(String, Expr)>),
}

impl Expr {
    /// Calls `f` on each direct subexpression, in evaluation order.
    pub fn each_child<'a>(&'a self, f: &mut dyn FnMut(&'a Expr)) {
        match &self.kind {
            ExprKind::Dicts(inner, _)
            | ExprKind::Abstract(_, inner)
            | ExprKind::Field(inner, _)
            | ExprKind::Not(inner)
            | ExprKind::Unwrap(inner)
            | ExprKind::Neg(_, inner)
            | ExprKind::Ignore(inner)
            | ExprKind::Await(inner)
            | ExprKind::Task(inner)
            | ExprKind::Scope { body: inner, .. }
            | ExprKind::Within(_, inner)
            | ExprKind::Lift(inner, _)
            | ExprKind::Lambda { body: inner, .. }
            | ExprKind::Yield(inner) => f(inner),
            ExprKind::Generate { body, limit, .. } => {
                if let Some(limit) = limit {
                    f(limit);
                }
                f(body);
            }
            ExprKind::Each { source, body, .. } => {
                f(source);
                f(body);
            }
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                if *arg_first {
                    f(arg);
                    f(func);
                } else {
                    f(func);
                    f(arg);
                }
            }
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let(_, v) | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => f(v),
                    }
                }
                f(result);
            }
            ExprKind::If(c, t, e) => {
                f(c);
                f(t);
                f(e);
            }
            ExprKind::Match(s, arms) | ExprKind::Try(s, arms) => {
                f(s);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        f(g);
                    }
                    f(&arm.body);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    f(item);
                }
            }
            ExprKind::Record(fields) | ExprKind::CodecBlock(_, fields) => {
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Update(r, fields) => {
                f(r);
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Binary(_, l, r) | ExprKind::Provide(l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. }
            | ExprKind::Con(_)
            | ExprKind::Operation { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => {}
        }
    }
}

impl Expr {
    /// Calls `f` on each direct subexpression, mutably, in evaluation order.
    pub fn each_child_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        match &mut self.kind {
            ExprKind::Dicts(inner, _)
            | ExprKind::Abstract(_, inner)
            | ExprKind::Field(inner, _)
            | ExprKind::Not(inner)
            | ExprKind::Unwrap(inner)
            | ExprKind::Neg(_, inner)
            | ExprKind::Ignore(inner)
            | ExprKind::Await(inner)
            | ExprKind::Task(inner)
            | ExprKind::Scope { body: inner, .. }
            | ExprKind::Within(_, inner)
            | ExprKind::Lift(inner, _)
            | ExprKind::Lambda { body: inner, .. }
            | ExprKind::Yield(inner) => f(inner),
            ExprKind::Generate { body, limit, .. } => {
                if let Some(limit) = limit {
                    f(limit);
                }
                f(body);
            }
            ExprKind::Each { source, body, .. } => {
                f(source);
                f(body);
            }
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                if *arg_first {
                    f(arg);
                    f(func);
                } else {
                    f(func);
                    f(arg);
                }
            }
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    match stmt {
                        Stmt::Let(_, v) | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => f(v),
                    }
                }
                f(result);
            }
            ExprKind::If(c, t, e) => {
                f(c);
                f(t);
                f(e);
            }
            ExprKind::Match(s, arms) | ExprKind::Try(s, arms) => {
                f(s);
                for arm in arms {
                    if let Some(g) = &mut arm.guard {
                        f(g);
                    }
                    f(&mut arm.body);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    f(item);
                }
            }
            ExprKind::Record(fields) | ExprKind::CodecBlock(_, fields) => {
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Update(r, fields) => {
                f(r);
                for (_, v) in fields {
                    f(v);
                }
            }
            ExprKind::Binary(_, l, r) | ExprKind::Provide(l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. }
            | ExprKind::Con(_)
            | ExprKind::Operation { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct Param {
    pub pat: Pat,
    /// The stage of the arrow of this parameter.
    pub stage: Stage,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(Pat, Expr),
    /// A local function; `rec` when it may call itself.
    Fun {
        name: String,
        rec: bool,
        dicts: Vec<String>,
        value: Expr,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i32),
    Float(f64),
    /// Decimal or `0x`/`0o`/`0b` digits with an optional `-`, without `_`
    /// or a suffix: its size is not bounded.
    BigInt(String),
    String(String),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        Expr { kind, span }
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

#[cfg(test)]
pub(crate) mod tests;
