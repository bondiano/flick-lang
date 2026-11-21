//! Expression holes (`f a _ c`), typed as `fun h -> f a h c` with the
//! parameter keyed at the hole's span.

use super::Checker;
use super::ty::Type;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The direct `_` hole argument of an application, if it has one.
pub fn app_hole(node: &Node) -> Option<&Node> {
    if node.kind != K::AppExpr {
        return None;
    }
    node.nodes().skip(1).find(|n| n.kind == K::HoleExpr)
}

/// Whether `node` is an application containing a direct `_` hole argument.
pub fn has_hole(node: &Node) -> bool {
    app_hole(node).is_some()
}

impl Checker<'_> {
    /// `f a _ c`, typed as `fun h -> f a h c` with `h` at the hole's span.
    pub(super) fn apply_hole(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let children: Vec<&Node> = node.nodes().collect();
        let Some((&head, written)) = children.split_first() else {
            return Type::Error;
        };
        let arguments = super::children::arguments(written);
        let Some(hole) = app_hole(node) else {
            return self.apply(head, &arguments);
        };
        let hole_span = self.span(hole);
        let hole_range = hole.range();
        let node_span = self.span(node);
        let node_range = node.range();
        let mut body = |c: &mut Self, param: Option<&Type>, rest: Option<Type>| {
            let param = param.cloned().unwrap_or(Type::Error);
            c.note_type(hole_span, &param);
            c.note_value(hole_range, &param);
            let index = c.cx.locals.len();
            c.push_local("$hole", hole_range.0, param);
            c.cx.hole_locals.push((hole_range, index));
            let value = c.apply(head, &arguments);
            c.cx.hole_locals.pop();
            match rest {
                Some(rest) => {
                    let prev_lift = c.typing.lifts.remove(&node_range);
                    let prev_coercion = c.typing.coercions.remove(&node_range);
                    c.expect(&value, &rest, node_span);
                    if let Some(lift) = c.typing.lifts.remove(&node_range) {
                        c.typing.hole_body_lifts.insert(node_range, lift);
                    }
                    if let Some(coercion) = c.typing.coercions.remove(&node_range) {
                        c.typing.hole_body_coercions.insert(node_range, coercion);
                    }
                    if let Some(lift) = prev_lift {
                        c.typing.lifts.insert(node_range, lift);
                    }
                    if let Some(coercion) = prev_coercion {
                        c.typing.coercions.insert(node_range, coercion);
                    }
                    rest
                }
                None => value,
            }
        };
        self.function_typed(&[None], &mut body, expected, node)
    }

    /// The type of `_` inside an application hole capture.
    pub(super) fn hole_expr(&mut self, node: &Node) -> Type {
        let span = self.span(node);
        let key = node.range();
        let Some(&(_, index)) = self.cx.hole_locals.iter().rev().find(|(at, _)| *at == key) else {
            return Type::Error;
        };
        let ty = self.use_local(index, span);
        self.value_bound(ty)
    }
}
