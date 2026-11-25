//! Paths of fields: `user.address.city` is a left-nested `FieldExpr` per
//! step, and the accessor `.address.city` an `AccessorExpr` of
//! `AccessorStep`s. A step written `.?label` keeps its `?` as a token of
//! its own node, apart from the propagation `user?.city`, whose `?` is a
//! `QuestionExpr` below the step.

use super::cst::Node;
use super::kind::SyntaxKind as K;

/// Whether `step`, a `FieldExpr` or an `AccessorStep`, is written `.?label`.
pub fn is_optional(step: &Node) -> bool {
    step.has_token(K::Question)
}

/// The steps of the rooted path ending at the `FieldExpr` `node`, root
/// first, and its root, which the parser may have left out: those of
/// `user.a.?b` are `.a` and `.?b`, its root `user`. Parentheses, a `?` and
/// an application end a path, as its root.
pub fn steps(node: &Node) -> (Option<&Node>, Vec<&Node>) {
    let mut steps = vec![node];
    let mut root = node.nodes().next();
    while let Some(lhs) = root.filter(|n| n.kind == K::FieldExpr) {
        steps.push(lhs);
        root = lhs.nodes().next();
    }
    steps.reverse();
    (root, steps)
}
