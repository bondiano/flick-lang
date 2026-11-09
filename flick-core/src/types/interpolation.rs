//! `"text \{e} text"`: each hole is `display e` of the Prelude's
//! `Display`, whatever names are in scope, and the string is the parts
//! joined by `++`. The lowering reads the dictionaries at [`hole_key`].

use super::Checker;
use super::ty::Type;
use crate::source::Span;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

/// Where the `display` of a hole is recorded: from its `\{` to the end of
/// its expression, a range no expression has.
pub fn hole_key(opener: &Token, hole: &Node) -> (u32, u32) {
    (opener.end - 2, hole.range().1)
}

/// The holes of a string with the token that opens each, in order; a hole
/// without an expression, already reported, is left out.
pub fn holes(node: &Node) -> Vec<(Token, &Node)> {
    let mut out = Vec::new();
    let mut opener = None;
    for child in &node.children {
        match child {
            Element::Token(t) if matches!(t.kind, K::StringStart | K::StringMid) => {
                opener = Some(*t);
            }
            Element::Node(hole) => {
                let code = hole.tokens().iter().any(|t| !t.kind.is_trivia());
                if let Some(opener) = opener.take().filter(|_| code) {
                    out.push((opener, hole));
                }
            }
            Element::Token(_) => {}
        }
    }
    out
}

impl Checker<'_> {
    pub(super) fn interpolation(&mut self, node: &Node) -> Type {
        let string = Type::lang(Lang::String);
        for (opener, hole) in holes(node) {
            let (start, end) = hole_key(&opener, hole);
            let span = Span::new(self.file, start, end);
            let display = self.prelude_method(Lang::DisplayMethod, span);
            let arg = self.infer(hole);
            self.expect(&display, &Type::pure_fun(arg, string.clone()), span);
        }
        string
    }
}
