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
mod splice;
mod wit;

use serde::{Deserialize, Serialize};

pub use builder::Builder;
pub use cursor::Cursor;
pub use flick_plugin_macros::flick;
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

/// A block `head args { body }`: the text from its first argument (or from
/// its body, without arguments) to the closing `}`, where the body starts in
/// it, and the tokens of the arguments and the body, `{` left out. All spans
/// count from the start of `text`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block {
    pub head: String,
    pub text: String,
    pub body: u32,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HoleKind {
    Expr,
    Ty,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Expr(Expr),
    Ty(Ty),
    Pat(Pat),
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
}

/// A plugin: what it learns from all its blocks of a group, and the code
/// each block stands for. Both are pure, so the compiler caches them.
pub trait Plugin {
    fn collect(&self, blocks: &[Block]) -> Vec<u8>;
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
    /// What an editor shows at `offset` of `block`.
    fn hover(&self, block: &Block, state: &[u8], offset: u32) -> Option<Hover> {
        let _ = (block, state, offset);
        None
    }
    /// How an editor colors the words of `block` that are the plugin's.
    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        let _ = (block, state);
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
