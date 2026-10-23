//! Trivia-free view of the concrete tree: what the program means syntactically,
//! independent of spacing, line breaks, comments and separators that change
//! nothing: a trailing one (`[a, b,]` is `[a, b]`) and `;` in records, where
//! a line break separates fields too. `fmt` must preserve it.

use super::cst::{Element, Node};
use super::kind::SyntaxKind;
use super::kind::SyntaxKind as K;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    Node(SyntaxKind, Vec<Ast>),
    Token(SyntaxKind, String),
}

pub fn lower(node: &Node, src: &str) -> Ast {
    let solid: Vec<&Element> = node
        .children
        .iter()
        .filter(|c| !matches!(c, Element::Token(t) if t.kind.is_trivia()))
        .collect();
    // in records `;` and a line break are the same separator
    let record = matches!(
        node.kind,
        K::RecordExpr | K::RecordUpdate | K::RecordType | K::RecordPat
    );
    let trailing = |i: usize| match solid[i] {
        Element::Token(t) if t.kind == K::Semi && record => true,
        Element::Token(t) if matches!(t.kind, K::Comma | K::Semi) => {
            matches!(solid.get(i + 1), Some(Element::Token(t)) if t.kind.is_closer())
        }
        _ => false,
    };
    let children = solid
        .iter()
        .enumerate()
        .filter(|&(i, _)| !trailing(i))
        .map(|(_, child)| match child {
            Element::Node(n) => lower(n, src),
            Element::Token(t) => Ast::Token(t.kind, t.text(src).to_owned()),
        })
        .collect();
    Ast::Node(node.kind, children)
}
