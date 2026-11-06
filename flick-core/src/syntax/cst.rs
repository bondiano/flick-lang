//! Lossless concrete syntax tree: every byte of the source, trivia included,
//! belongs to exactly one token, so `tree.text(src) == src`.

use std::fmt::Write;

use super::kind::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub start: u32,
    pub end: u32,
}

impl Token {
    pub fn text(self, src: &str) -> &str {
        &src[self.start as usize..self.end as usize]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    Node(Node),
    Token(Token),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: SyntaxKind,
    pub children: Vec<Element>,
}

impl Node {
    /// All tokens in source order.
    pub fn tokens(&self) -> Vec<Token> {
        fn walk(node: &Node, out: &mut Vec<Token>) {
            for child in &node.children {
                match child {
                    Element::Node(n) => walk(n, out),
                    Element::Token(t) => out.push(*t),
                }
            }
        }
        let mut out = Vec::new();
        walk(self, &mut out);
        out
    }

    pub fn text(&self, src: &str) -> String {
        self.tokens().iter().map(|t| t.text(src)).collect()
    }

    /// Direct child tokens that are not trivia.
    pub fn child_tokens(&self) -> impl Iterator<Item = Token> + '_ {
        self.children.iter().filter_map(|c| match c {
            Element::Token(t) if !t.kind.is_trivia() => Some(*t),
            _ => None,
        })
    }

    pub fn has_token(&self, kind: SyntaxKind) -> bool {
        self.child_tokens().any(|t| t.kind == kind)
    }

    /// Byte range of the non-trivia tokens; `0..0` for a node without any.
    pub fn range(&self) -> (u32, u32) {
        let tokens = self.tokens();
        let mut code = tokens.iter().filter(|t| !t.kind.is_trivia());
        let first = code.next();
        let last = code.next_back().or(first);
        (first.map_or(0, |t| t.start), last.map_or(0, |t| t.end))
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|c| match c {
            Element::Node(n) => Some(n),
            Element::Token(_) => None,
        })
    }

    /// Indented S-expression dump without trivia, for tests and debugging.
    pub fn debug_dump(&self, src: &str) -> String {
        fn walk(node: &Node, src: &str, depth: usize, out: &mut String) {
            let _ = writeln!(out, "{}{:?}", "  ".repeat(depth), node.kind);
            for child in &node.children {
                match child {
                    Element::Node(n) => walk(n, src, depth + 1, out),
                    Element::Token(t) if !t.kind.is_trivia() => {
                        let indent = "  ".repeat(depth + 1);
                        let _ = writeln!(out, "{indent}{:?} {:?}", t.kind, t.text(src));
                    }
                    Element::Token(_) => {}
                }
            }
        }
        let mut out = String::new();
        walk(self, src, 0, &mut out);
        out
    }
}

/// The words of `foreign [mode] name …`: its mode, when written, and its
/// name. Without a mode the signature tells how the host is called.
pub fn foreign_words(decl: &Node) -> (Option<Token>, Option<Token>) {
    let idents: Vec<Token> = (decl.child_tokens())
        .filter(|t| t.kind == SyntaxKind::Ident)
        .collect();
    match idents[..] {
        [mode, name, ..] => (Some(mode), Some(name)),
        [name] => (None, Some(name)),
        [] => (None, None),
    }
}
