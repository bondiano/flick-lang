//! Trivia-free view of the concrete tree: what the program means syntactically,
//! independent of spacing, line breaks, comments and separators that change
//! nothing: a trailing one (`[a, b,]` is `[a, b]`) and `;` in records, where
//! a line break separates fields too, also as a plugin block's tokens.
//! `fmt` must preserve it.

use super::cst::{Element, Node};
use super::kind::SyntaxKind;
use super::kind::SyntaxKind as K;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    Node(SyntaxKind, Vec<Ast>),
    Token(SyntaxKind, String),
}

pub fn lower(node: &Node, src: &str) -> Ast {
    let trivia = |c: &Element| matches!(c, Element::Token(t) if t.kind.is_trivia());
    let solid: Vec<(usize, &Element)> = node
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| !trivia(c))
        .collect();
    // in records `;` and a line break are the same separator
    let record = matches!(
        node.kind,
        K::RecordExpr | K::RecordUpdate | K::RecordType | K::RecordPat
    );
    // a plugin block holds its tokens unparsed: a `;` ending one of its
    // lines is that separator in the code of its holes
    let plugin = matches!(
        node.kind,
        K::PluginExpr | K::PluginDecl | K::PluginArgs | K::PluginRest
    );
    let ends_line = |raw: usize| {
        (node.children[raw + 1..].iter())
            .take_while(|c| trivia(c))
            .any(
                |c| matches!(c, Element::Token(t) if matches!(t.kind, K::Newline | K::LineComment)),
            )
    };
    let trailing = |i: usize| match solid[i].1 {
        Element::Token(t) if t.kind == K::Semi && record => true,
        Element::Token(t) if t.kind == K::Semi && plugin && ends_line(solid[i].0) => true,
        Element::Token(t) if matches!(t.kind, K::Comma | K::Semi) => {
            matches!(solid.get(i + 1), Some((_, Element::Token(t))) if t.kind.is_closer())
        }
        _ => false,
    };
    let children = solid
        .iter()
        .enumerate()
        .filter(|&(i, _)| !trailing(i))
        .map(|(_, (_, child))| match child {
            Element::Node(n) => lower(n, src),
            Element::Token(t) => Ast::Token(t.kind, t.text(src).to_owned()),
        })
        .collect();
    Ast::Node(node.kind, children)
}
