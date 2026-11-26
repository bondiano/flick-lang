//! `"text \{e} text"` as the checker reads it (`types::interpolation`):
//! the parts of the string and `display` of each hole, joined by `++` from
//! the left, so the holes run in order and the optimizer folds constants.

use std::ptr;

use fln_literals::decode_string_part;

use super::Lower;
use super::numeric::apply;
use crate::ir::{BinOp, Expr, ExprKind, Lit};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{hole_key, holes};

impl Lower<'_> {
    pub(super) fn interpolation(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let mut holes = holes(node).into_iter().peekable();
        let mut parts = Vec::new();
        for child in &node.children {
            match child {
                Element::Token(t)
                    if matches!(t.kind, K::StringStart | K::StringMid | K::StringEnd) =>
                {
                    let text = decode_string_part(t.text(self.text));
                    if !text.is_empty() {
                        parts.push(Expr::new(ExprKind::Lit(Lit::String(text)), span));
                    }
                }
                Element::Node(n) if holes.peek().is_some_and(|(_, h)| ptr::eq(*h, n)) => {
                    let (opener, hole) = holes.next().expect("peeked");
                    let display = self.prelude_method(hole_key(&opener, hole), Lang::DisplayMethod);
                    let value = self.expr(hole);
                    parts.push(apply(display, [Box::new(value)], span));
                }
                _ => {}
            }
        }
        let mut parts = parts.into_iter();
        let first = parts
            .next()
            .unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::String(String::new())), span));
        parts.fold(first, |l, r| {
            Expr::new(
                ExprKind::Binary(BinOp::Concat, Box::new(l), Box::new(r)),
                span,
            )
        })
    }
}
