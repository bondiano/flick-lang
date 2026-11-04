//! The interface of Flick compiler plugins. A plugin owns blocks `head { … }`
//! of the programs that import it: it reads their tokens, asks the compiler
//! to parse the user's code inside them (the holes), and gives back the code
//! a block stands for as an arena of core syntax, each node with the span of
//! the body it comes from.
//!
//! These types mirror `wit/plugin.wit`, the interface of plugins compiled to
//! WebAssembly components; plugins linked into the compiler implement
//! [`Plugin`] directly.

mod builder;

use serde::{Deserialize, Serialize};

pub use builder::Builder;

/// The version of the interface, as `wit/plugin.wit` declares it. Caches of
/// expansions key on it.
pub const VERSION: &str = "0.1.0";

/// A UTF-8 byte range in the body of a block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
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

/// A block `head { body }`: the text of its body and the tokens in it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block {
    pub head: String,
    pub text: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
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
