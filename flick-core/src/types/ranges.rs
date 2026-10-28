//! Ranges of numbers, each a `Seq Float`: `a..b` from `a` up to `b`
//! inclusive, `a..<b` up to before `b`, and `a..` without an end. A range
//! alone in brackets, `[a..b]`, is the `List` of the same numbers.

use super::Checker;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The range of `[a..b]`, if `list` is one.
pub fn range_list(list: &Node) -> Option<&Node> {
    let mut items = list.nodes();
    match (items.next(), items.next()) {
        (Some(only), None) if only.kind == K::RangeExpr => Some(only),
        _ => None,
    }
}

impl Checker<'_> {
    pub fn range_expr(&mut self, node: &Node, strict: bool) -> Type {
        let number = Type::prim("Int");
        let bounds: Vec<&Node> = node.nodes().collect();
        for bound in &bounds {
            self.check(bound, &number);
        }
        if strict && bounds.len() < 2 {
            self.error(
                codes::T003,
                self.span(node),
                "`[a..]` would never end: take a part of the sequence, as in `Seq.to_list (Seq.take n (a..))`",
            );
        }
        let collection = if strict { "List" } else { "Seq" };
        Type::app(Type::prim(collection), number)
    }
}
