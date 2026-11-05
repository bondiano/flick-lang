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
#[cfg(feature = "guest")]
pub mod guest;
mod literal;
mod reach;
mod splice;
mod wit;

use serde::{Deserialize, Serialize};

pub use builder::Builder;
pub use cursor::Cursor;
pub use flick_plugin_macros::flick;
pub use literal::string_value;
pub use reach::{ReachError, Request, Response};
#[doc(hidden)]
pub use splice::__private;
pub use splice::{Name, Splice, Wildcard};

/// The version of the interface, as `wit/plugin.wit` declares it. Caches of
/// expansions key on it.
pub const VERSION: &str = "0.12.0";

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
    /// `type Name params = A fields | … deriving C, …`.
    Adt {
        name: String,
        params: Vec<String>,
        cases: Vec<Case>,
        deriving: Vec<String>,
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

/// An error in a block; `code` is the plugin's own, such as `L001`, which
/// the compiler shows under the plugin's package: `std/L001`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub code: Option<String>,
}

impl Diagnostic {
    /// An error without a code of the plugin.
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Diagnostic {
            message: message.into(),
            span,
            code: None,
        }
    }

    /// This error under the plugin's code `code`, such as `L001`.
    #[must_use]
    pub fn with_code(self, code: &str) -> Self {
        Diagnostic {
            code: Some(code.to_owned()),
            ..self
        }
    }
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
    /// The base type of a `refined type`.
    pub refined: Option<Span>,
    pub local: bool,
}

/// A constructor of a derive's target and the types of its fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseTarget {
    pub name: String,
    pub fields: Vec<Span>,
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

/// A type the checker found, as it shows it and qualified with aliases
/// expanded; `exact` is none for a type with errors or still unknown parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactType {
    pub shown: String,
    pub exact: Option<String>,
}

/// A field of a record a fact's type is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactField {
    pub label: String,
    pub ty: FactType,
}

/// The type of a `_` of a block's code, at the span its code gave it, and
/// its fields when it is a closed record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub span: Span,
    pub ty: FactType,
    pub fields: Option<Vec<FactField>>,
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
}

/// A plugin: what it learns from all its blocks of a group, and the code
/// each block stands for. Both depend only on the blocks and on what the
/// project grants them to read, so the compiler caches each result while
/// what its call read stays the same.
pub trait Plugin {
    /// What the plugin learns from `blocks`, the state each expansion
    /// gets; `compiler` reaches what the project grants, but parses no
    /// hole.
    fn collect(&self, blocks: &[Block], compiler: &mut dyn Compiler) -> Vec<u8>;
    /// The code `block` stands for given what `collect` learned, or why
    /// there is none; no diagnostic means the compiler already reported why.
    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>>;
    /// What an editor may insert at `offset` of `block`.
    fn complete(&self, block: &Block, state: &[u8], offset: u32) -> Vec<Completion> {
        let _ = (block, state, offset);
        Vec::new()
    }
    /// What an editor shows at `offset` of `block`; `facts` are the types
    /// of the `_` of its code, as `check` gets them, once it checked.
    fn hover(&self, block: &Block, state: &[u8], offset: u32, facts: &[Fact]) -> Option<Hover> {
        let _ = (block, state, offset, facts);
        None
    }
    /// How an editor colors the words of `block` that are the plugin's.
    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        let _ = (block, state);
        Vec::new()
    }
    /// The instance of `class` for `target`, and what goes with it, as
    /// declarations; `args` is the `{ … }` after the class in `deriving`,
    /// and `module` every type the module of `target` declares.
    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let _ = (args, module, state, compiler);
        let message = format!("`{class}` has no derive in this plugin");
        Err(vec![Diagnostic::error(message, target.decl.whole())])
    }
    /// The layout of `block`, the user's code in it found through
    /// `compiler`; none keeps the block as written.
    fn format(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Option<Vec<Piece>> {
        let _ = (block, state, compiler);
        None
    }
    /// What is wrong with `block` that only the types of its code show:
    /// `facts` are the types of the `_` of its code, at the spans it gave
    /// them. It only adds errors.
    fn check(&self, block: &Block, state: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
        let _ = (block, state, facts);
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
