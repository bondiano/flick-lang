//! A structured Lua 5.1 program: what the lowering builds and the printer
//! writes. Every node carries the Flick span it was lowered from.
//!
//! The shapes keep Lua's rules out of reach of a mistake:
//!
//! - A [`Block`] ends in at most one [`Last`], a `return` or a `break`, and
//!   no statement can follow it.
//! - Every expression list (arguments, table items, a `return` or a local's
//!   value) wants one value per position. Lua passes every result of a call
//!   at the end of a list, so a call that may return other than one value,
//!   [`Results::Any`], is written in parentheses there; a call of a Flick
//!   function, [`Results::One`], is not, so `return f(x)` stays a tail call.
//! - There is no `goto`: Lua 5.1 has none, and control flow is structured.

use flick_ir::Span;

/// A Lua identifier the lowering gave one binding: never a reserved word,
/// never a name another binding of the same top-level definition has, nor
/// a global the generated code reads (see [`super::names`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    /// A name the caller has checked: an identifier, not a keyword.
    pub(super) fn new(text: String) -> Name {
        debug_assert!(super::names::is_identifier(&text), "{text}");
        Name(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// How many values a call returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Results {
    /// Exactly one, as every Flick function does.
    One,
    /// Any number, as a host function may.
    Any,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Nil,
    Bool(bool),
    /// A number literal that reads back as exactly the value meant: digits
    /// with an optional fraction and exponent, never a sign.
    Number(String),
    /// A string of the bytes of this text.
    Str(String),
    Name(Name),
    /// `object[key]`, written `object.key` when the key is a string that is
    /// an identifier.
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>, Results),
    Function(Box<Function>),
    Table(Vec<Field>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnOp, Box<Expr>),
}

/// An entry of a table constructor.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    /// The next item of the array part.
    Item(Expr),
    /// `[key] = value`, written `key = value` for a string key that is an
    /// identifier.
    Pair(Expr, Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
    Len,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub params: Vec<Name>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `local name = value`, or `local name` without one.
    Local(Name, Option<Expr>),
    /// `local function name(params) … end`, which sees itself.
    LocalFunction(Name, Function),
    /// `target = value`; the target is a name or an index.
    Assign(Expr, Expr),
    /// A call for its effects; the expression is a call.
    Call(Expr),
    Do(Block),
    While(Expr, Block),
    /// `repeat … until condition`; the condition sees the block's locals.
    Repeat(Block, Expr),
    /// `if c then … elseif c then … else … end`.
    If(Vec<(Expr, Block)>, Option<Block>),
    /// `for name = start, stop do … end`.
    For(Name, Expr, Expr, Block),
    /// `for key, value in pairs(table) do … end`.
    Pairs(Name, Name, Expr, Block),
}

/// Statements and what ends them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub last: Option<Last>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Last {
    pub kind: LastKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LastKind {
    Return(Expr),
    Break,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Expr {
        Expr { kind, span }
    }

    pub fn name(name: &Name, span: Span) -> Expr {
        Expr::new(ExprKind::Name(name.clone()), span)
    }

    pub fn str(text: &str, span: Span) -> Expr {
        Expr::new(ExprKind::Str(text.to_owned()), span)
    }

    pub fn number(n: usize, span: Span) -> Expr {
        Expr::new(ExprKind::Number(n.to_string()), span)
    }

    /// `self[key]`.
    pub fn index(self, key: Expr) -> Expr {
        let span = self.span;
        Expr::new(ExprKind::Index(Box::new(self), Box::new(key)), span)
    }

    /// `self.label`.
    pub fn field(self, label: &str) -> Expr {
        let span = self.span;
        self.index(Expr::str(label, span))
    }

    /// `self(args)`, a call of a Flick function.
    pub fn call(self, args: Vec<Expr>) -> Expr {
        let span = self.span;
        Expr::new(ExprKind::Call(Box::new(self), args, Results::One), span)
    }

    pub fn binary(op: BinOp, lhs: Expr, rhs: Expr, span: Span) -> Expr {
        Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span)
    }

    pub fn unary(op: UnOp, inner: Expr, span: Span) -> Expr {
        Expr::new(ExprKind::Unary(op, Box::new(inner)), span)
    }
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Stmt {
        Stmt { kind, span }
    }
}

impl Block {
    /// Appends `stmt`, unless the block has ended: code after a `return`
    /// or a `break` is never reached, and Lua does not parse it.
    pub fn push(&mut self, stmt: Stmt) {
        if self.last.is_none() {
            self.stmts.push(stmt);
        }
    }

    /// Ends the block, unless it has ended already.
    pub fn end(&mut self, kind: LastKind, span: Span) {
        if self.last.is_none() {
            self.last = Some(Last { kind, span });
        }
    }

    /// Appends the statements of `other`, and what ends it.
    pub fn append(&mut self, other: Block) {
        for stmt in other.stmts {
            self.push(stmt);
        }
        if let Some(last) = other.last {
            self.end(last.kind, last.span);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty() && self.last.is_none()
    }
}
