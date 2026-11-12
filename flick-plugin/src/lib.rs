//! The interface of Flick compiler plugins. A plugin owns blocks `head { … }`
//! of the programs that import it: it reads their tokens, asks the compiler
//! to parse the user's code inside them (the holes), and gives back the code
//! a block stands for as an arena of core syntax, each node with the span of
//! the body it comes from.
//!
//! These types mirror `wit/plugin.wit`, the interface of plugins compiled to
//! WebAssembly components; plugins linked into the compiler implement
//! [`Plugin`] directly.

// the paths `flick!` writes, inside this crate too
extern crate self as flick_plugin;

mod builder;
mod cursor;
mod diagnostic;
mod exports;
mod facts;
#[cfg(feature = "guest")]
pub mod guest;
mod literal;
mod reach;
mod splice;
mod wit;
mod wit_arena;
mod wit_calls;

use serde::{Deserialize, Serialize};

pub use builder::Builder;
pub use cursor::Cursor;
pub use diagnostic::{Diagnostic, Edit, Fix, Severity};
pub use exports::Plugin;
pub use facts::{
    Argument, Binding, CallSite, Constant, Enclosing, ExpressionFact, ExpressionKind, Fact,
    FactField, FactType, Lacking, Nesting, Refinement, Structure, Tagged,
};
pub use flick_plugin_macros::flick;
pub use literal::{literal_text, string_value};
pub use reach::{ReachError, Request, Response};
#[doc(hidden)]
pub use splice::__private;
pub use splice::{Name, Splice, Wildcard};

/// The version of the interface, as `wit/plugin.wit` declares it. Caches of
/// expansions key on it.
pub const VERSION: &str = "0.2.0";

/// A UTF-8 byte range in the body of a block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl From<&Span> for Span {
    fn from(span: &Span) -> Self {
        *span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenKind {
    Ident,
    Con,
    Number,
    String,
    Keyword,
    Punct,
}

/// A token of the body, trivia left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Where a block stands: in an expression, whose code is an expression, or
/// as a declaration of the module, whose code is declarations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Place {
    #[default]
    Expression,
    Declaration,
}

/// A block `head args { body }`, `head args` or, as a declaration, `head
/// args rest`: the text from its first argument (or from its body, without
/// arguments) to the closing `}` or its end, where the body starts in it
/// (its length without one), and the tokens of the arguments, the body and
/// the rest, `{` left out. All spans count from the start of `text`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block {
    pub head: String,
    pub text: String,
    pub body: u32,
    pub place: Place,
    pub tokens: Vec<Token>,
    /// The `///` lines right above a declaration's head, each without `///`
    /// and one space, joined by `\n`; empty for an expression.
    pub docs: String,
    /// The signature right above a declaration, if there is one.
    pub signature: Option<Signature>,
    /// The module the block is in, as `App.Shop`.
    pub module: String,
}

/// The signature `name : type` right above a declaration block, its spans
/// counting from its own start. Code that has a type of it, [`Ty::Signed`],
/// takes it from the module; without that it stays where it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signature {
    pub text: String,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HoleKind {
    Expr,
    Ty,
    Pat,
    Decl,
    /// A body whose calls take the lines under them as arguments, one
    /// each: an expression's hole otherwise.
    Lines,
}

/// User code in the body that the compiler parsed: an expression or a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hole {
    pub kind: HoleKind,
    pub span: Span,
}

/// The index of a node in its arena; a node refers only to nodes before it.
pub type NodeId = u32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub label: String,
    pub value: NodeId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Name(String),
    Number(f64),
    Text(String),
    Boolean(bool),
    Unit,
    Apply {
        function: NodeId,
        arguments: Vec<NodeId>,
    },
    Lambda {
        params: Vec<NodeId>,
        body: NodeId,
    },
    Record(Vec<Field>),
    Get {
        record: NodeId,
        label: String,
    },
    Items(Vec<NodeId>),
    Tuple(Vec<NodeId>),
    Annot {
        expr: NodeId,
        ty: NodeId,
    },
    /// The user's expression at this span of the body.
    Hole(Span),
    /// The user's body at this span, read with the lines under a call as
    /// its arguments, one each: what [`Compiler::parse_lines`] gave.
    Lines(Span),
    /// `(a ++ b ++ …)`: texts or lists joined, at least two.
    Concat(Vec<NodeId>),
    // shortcut: `flick!` writes no `match`, `if` or block, which `Builder` builds;
    // add the syntax when plugins write them by hand often
    /// `match scrutinee with | p when g -> e …`, at least one arm.
    Match {
        scrutinee: NodeId,
        arms: Vec<Arm>,
    },
    /// `if condition then then else otherwise`.
    If {
        condition: NodeId,
        then: NodeId,
        otherwise: NodeId,
    },
    /// Statements, then the value; only the body of a `fun` or a `let`. A
    /// statement is a local `let`, a signature, a hole of one, or an
    /// expression.
    Block {
        statements: Vec<NodeId>,
        result: NodeId,
    },
    /// `body` with the module `module` open: a name the module exports is
    /// that one there, before the names of the file, unless a local of the
    /// body hides it. No source writes this; a block that brings the names
    /// of its library into sight stands for it. With `child`, a type, a
    /// call there of a function whose last parameter is a list of it gives
    /// that list element by element: each argument from that parameter on
    /// is one, and none is the empty list.
    Open {
        module: String,
        child: Option<String>,
        body: NodeId,
    },
}

/// An arm of a `match`: its pattern, maybe a guard, and its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arm {
    pub pattern: NodeId,
    pub guard: Option<NodeId>,
    pub body: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ty {
    Name(String),
    Apply {
        function: NodeId,
        arguments: Vec<NodeId>,
    },
    Record(Vec<Field>),
    Function {
        input: NodeId,
        output: NodeId,
    },
    /// A type-level string, as in `Record.Label "name"`.
    Text(String),
    /// `_`: the type the checker infers.
    Wildcard,
    /// The user's type at this span of the body.
    Hole(Span),
    /// `forall a b. body`.
    Forall {
        vars: Vec<String>,
        body: NodeId,
    },
    /// A row of types, as the errors `{NotFound, Conflict}`; `{}` without.
    Row(Vec<NodeId>),
    /// The user's type at this span of the block's signature.
    Signed(Span),
    /// `(A, B)`.
    Tuple(Vec<NodeId>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pat {
    Var(String),
    Wildcard,
    Con {
        name: String,
        arguments: Vec<NodeId>,
    },
    Record(Vec<Field>),
    /// The user's pattern at this span of the block.
    Hole(Span),
    /// `(a, b)`.
    Tuple(Vec<NodeId>),
}

/// A constructor of a type and the types of its fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub name: String,
    pub fields: Vec<NodeId>,
}

/// A declaration of the module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decl {
    /// `let [rec] name params : ty = body`; the parameters are patterns, and
    /// a `rec` one may name itself.
    Let {
        name: String,
        params: Vec<NodeId>,
        ty: Option<NodeId>,
        body: NodeId,
        #[serde(default)]
        rec: bool,
    },
    /// `name : ty`.
    Signature { name: String, ty: NodeId },
    /// `type alias Name params = ty`.
    Alias {
        name: String,
        params: Vec<String>,
        ty: NodeId,
    },
    /// `type Name params = A fields | … deriving C, …`. An `unboxed` one has
    /// one constructor with one field, and a value of it is the field's value
    /// when the program runs: a type of its own over another, for nothing.
    Adt {
        name: String,
        params: Vec<String>,
        cases: Vec<Case>,
        deriving: Vec<String>,
        #[serde(default)]
        unboxed: bool,
    },
    /// `instance context => head = members`, the members `let`s and
    /// signatures.
    Instance {
        context: Vec<NodeId>,
        head: NodeId,
        members: Vec<NodeId>,
    },
    /// Declarations in order: the code of a block that gives several.
    Group(Vec<NodeId>),
    /// The user's declaration at this span of the block.
    Hole(Span),
    /// A declaration with its documentation, lines joined by `\n`.
    Doc { text: String, decl: NodeId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Expr(Expr),
    Ty(Ty),
    Pat(Pat),
    Decl(Decl),
}

/// A node of generated code and the span of the body it stands for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub kind: Kind,
    pub origin: Span,
}

/// Generated code: its nodes, each after the nodes it refers to, and the
/// node the block stands for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arena {
    pub nodes: Vec<Node>,
    pub root: NodeId,
}

/// A file a build writes: its path in the output directory and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub contents: String,
}

/// What an editor may insert at an offset of a block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    pub label: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    /// The text to insert, with `${1:placeholder}` stops.
    pub snippet: Option<String>,
}

/// A type a class's derive makes an instance for: its declaration as a
/// block, a declaration of the module, and what it declares, at spans of
/// that block. A type of another module (`deriving instance`) is not
/// `local`: its spans are no holes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeriveTarget {
    pub decl: Block,
    /// Its name, qualified for a type of another module: `Other.Color`.
    pub name: String,
    pub params: Vec<String>,
    /// Its constructors, qualified as its name is, with their fields' types.
    pub cases: Vec<CaseTarget>,
    /// The type a `type alias` stands for.
    pub alias: Option<Span>,
    /// The type its values are made of, for a type over another whose
    /// constructor is its declaration's alone: a `refined type`, or an
    /// `unboxed type` of a block with a constructor only generated code can
    /// name. Its values come from `T.parse` and are read by `T.value`.
    pub wraps: Option<Span>,
    pub local: bool,
}

/// A constructor of a derive's target, the types of its fields and the
/// literal of an enum's constructor: `"GET"` of `Get = "GET"`. A
/// constructor of a GADT names the type it builds (`refined`, as
/// `| IntLit Int : Expr Int`) or holds dictionaries (`constrained`, as
/// `| Show b => Showable b : Showable`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseTarget {
    pub name: String,
    pub fields: Vec<Span>,
    pub literal: Option<Span>,
    #[serde(default)]
    pub refined: bool,
    #[serde(default)]
    pub constrained: bool,
}

/// A piece of the layout a plugin gives a block, from its first argument
/// (or its body) on: the compiler writes the head before it, `{` before a
/// layout that starts in the body, and `}` after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Piece {
    /// The plugin's own words, on the current line.
    Text(String),
    /// A new line at the current indent.
    Line,
    /// One level more indent for the lines after it.
    Indent,
    /// One level less.
    Dedent,
    /// The user's code there, which the compiler lays out.
    Hole(Hole),
}

/// What an editor shows over a span of a block, as Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hover {
    pub markdown: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightKind {
    Keyword,
    Parameter,
}

/// How an editor colors a span of a block that is not the user's code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Highlight {
    pub span: Span,
    pub kind: HighlightKind,
}

/// The names of values or of types, as `Compiler::in_scope` asks for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Namespace {
    Value,
    Type,
}

/// What a plugin tells its blocks of other groups and modules about a name
/// of the module `module`, as `Compiler::published` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Published {
    pub module: String,
    pub name: String,
    pub data: Vec<u8>,
}

/// What the compiler offers a plugin while it expands a block.
pub trait Compiler {
    /// The longest expression of the body from `start` on, before `end`.
    fn parse_expr(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic>;
    /// The longest type of the body from `start` on, before `end`.
    fn parse_type(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic>;
    /// The longest pattern of the body from `start` on, before `end`.
    fn parse_pat(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic>;
    /// The declaration of the block from `start` on, before `end`.
    fn parse_decl(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic>;
    /// The body of the block from `start` on, before `end`, as after the
    /// `=` of a `let`: an expression, or on a line of its own the lines of
    /// a block at its column; its hole is an expression's.
    fn parse_body(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        self.parse_expr(start, end)
    }
    /// [`Compiler::parse_body`] where the lines under a call are its
    /// arguments, one expression each, and the lines under one of those its
    /// own: `div props`, then a line for each child. Its hole is of
    /// [`HoleKind::Lines`], which [`Expr::Lines`] stands for.
    fn parse_lines(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        self.parse_body(start, end)
    }
    /// The file at `path` from the project's root, as `[plugins.<package>]`
    /// grants it with `fs-read`.
    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, ReachError> {
        Err(ReachError::Denied(format!(
            "no project grants reading `{path}`"
        )))
    }
    /// The variable `name` of the environment, as `env` grants it.
    fn env(&mut self, name: &str) -> Result<Option<String>, ReachError> {
        Err(ReachError::Denied(format!(
            "no project grants reading `{name}`"
        )))
    }
    /// The answer of a host `net` grants, from the project's snapshots
    /// when the run is offline.
    fn fetch(&mut self, request: &Request) -> Result<Response, ReachError> {
        Err(ReachError::Denied(format!(
            "no project grants fetching `{}`",
            request.url
        )))
    }
    /// The base type, `String` or `Int`, of the enum `name` as the block's
    /// module writes it, `Role` or `Roles.Role`; `None` for a type that is
    /// no enum, or one the module does not see.
    fn literal_of(&mut self, name: &str) -> Option<String> {
        let _ = name;
        None
    }
    /// What this plugin published of `name` as the block's module writes
    /// it, `Users` or `Schema.Users`: its own module's, or one it imports,
    /// from any group; `None` for a name it does not see or no publication.
    fn published(&mut self, name: &str) -> Option<Vec<u8>> {
        let _ = name;
        None
    }
    /// Every name `published` answers for the block's module, as it writes
    /// them.
    fn published_names(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// The values or types in scope of the block's module, as it writes
    /// them; with `fits`, the values whose type instantiates to that type,
    /// written as the module writes it or with names qualified in full. An
    /// editor answers it; while a block expands, nothing.
    fn in_scope(&mut self, namespace: Namespace, fits: Option<&str>) -> Vec<Completion> {
        let _ = (namespace, fits);
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn the_version_is_the_one_of_the_interface() {
        let wit = include_str!("../wit/plugin.wit");
        assert!(
            wit.starts_with(&format!("package flick:plugin@{VERSION};")),
            "wit/plugin.wit declares another version than {VERSION}"
        );
    }
}
