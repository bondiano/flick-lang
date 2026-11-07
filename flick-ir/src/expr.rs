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
            ExprKind::Match(s, arms) => {
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
            ExprKind::Record(fields) => {
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
            ExprKind::Binary(_, l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Handle(h) => h.each_child(f),
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. }
            | ExprKind::Con(_)
            | ExprKind::Perform { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => {}
        }
    }
}

impl Module {
    /// Calls `f` on the span of every item and expression.
    pub fn each_span_mut(&mut self, f: &mut dyn FnMut(&mut Span)) {
        for item in &mut self.items {
            f(&mut item.span);
            match &mut item.kind {
                ItemKind::Value { body, .. }
                | ItemKind::Destructure { body, .. }
                | ItemKind::Refined { check: body, .. } => body.each_span_mut(f),
                ItemKind::Instance { methods, .. } => {
                    for (_, body) in methods {
                        body.each_span_mut(f);
                    }
                }
                ItemKind::Foreign(foreign) => {
                    if let Some(catch) = &mut foreign.catch {
                        catch.each_span_mut(f);
                    }
                }
                ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            }
        }
    }
}

impl Expr {
    /// Calls `f` on the span of this expression and of every one inside.
    pub fn each_span_mut(&mut self, f: &mut dyn FnMut(&mut Span)) {
        f(&mut self.span);
        self.each_child_mut(&mut |child| child.each_span_mut(f));
    }

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
            ExprKind::Match(s, arms) => {
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
            ExprKind::Record(fields) => {
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
            ExprKind::Binary(_, l, r) => {
                f(l);
                f(r);
            }
            ExprKind::Handle(h) => h.each_child_mut(f),
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Global { .. }
            | ExprKind::Method { .. }
            | ExprKind::Con(_)
            | ExprKind::Perform { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Label(_) => {}
        }
    }
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
