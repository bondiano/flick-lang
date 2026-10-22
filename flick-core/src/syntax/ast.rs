//! Trivia-free view of the concrete tree: what the program means syntactically,
//! independent of spacing, line breaks and comments. `fmt` must preserve it.

use super::cst::{Element, Node};
use super::kind::SyntaxKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    Node(SyntaxKind, Vec<Ast>),
    Token(SyntaxKind, String),
}

pub fn lower(node: &Node, src: &str) -> Ast {
    let children = node
        .children
        .iter()
        .filter_map(|child| match child {
            Element::Node(n) => Some(lower(n, src)),
            Element::Token(t) if t.kind.is_trivia() => None,
            Element::Token(t) => Some(Ast::Token(t.kind, t.text(src).to_owned())),
        })
        .collect();
    Ast::Node(node.kind, children)
}
