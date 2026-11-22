//! Lowering of expression holes (`f a _ c`) to lambdas (`fun $hole -> f a $hole c`).

use super::expr::with_dicts;
use super::{Lower, type_name};
use crate::ir::{Expr, ExprKind, Param, Pat, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::{Range, StageKey};

impl Lower<'_> {
    /// `f a _ c` as `fun $hole -> f a $hole c`, keyed by the hole span.
    pub(super) fn app_hole(
        &mut self,
        node: &Node,
        children: &[&Node],
        hole: &Node,
        span: Span,
    ) -> Expr {
        let key = node.range();
        let entry = StageKey::Arrow {
            node: key,
            index: 0,
        };
        self.register_owner_entry(entry);
        let param_name = self.hidden("$hole");
        let prev = self.hole_params.insert(hole.range(), param_name.clone());
        let body = self.apply(children[0], &children[1..]);
        match prev {
            Some(old) => {
                self.hole_params.insert(hole.range(), old);
            }
            None => {
                self.hole_params.remove(&hole.range());
            }
        }
        let body = self.adapt_hole_body(key, body);
        let body = self.owner_entry(entry, body);
        let stage = self
            .typing
            .stages
            .get(&key)
            .and_then(|s| s.first())
            .copied()
            .unwrap_or(Stage::Pure);
        let ty = self.typing.values.get(&hole.range()).and_then(type_name);
        Expr::new(
            ExprKind::Lambda {
                params: vec![Param {
                    pat: Pat::Bind(param_name),
                    stage,
                    ty,
                }],
                body: Box::new(body),
            },
            span,
        )
    }

    fn adapt_hole_body(&self, key: Range, mut value: Expr) -> Expr {
        if let Some(lift) = self.typing.hole_body_lifts.get(&key).cloned() {
            let span = value.span;
            value = Expr::new(ExprKind::Lift(Box::new(value), lift), span);
        }
        if let Some(coercion) = self.typing.hole_body_coercions.get(&key).cloned() {
            let span = value.span;
            value = Self::coercion_expr(value, coercion.dict, coercion.method, span);
        }
        value
    }

    /// An expression hole lowered to the parameter local of its enclosing
    /// application capture.
    pub(super) fn hole_expr(&mut self, node: &Node, span: Span) -> Expr {
        let unique = self
            .hole_params
            .get(&node.range())
            .expect("the checker resolved an expression hole")
            .clone();
        let dicts = self.dictionaries(node.range());
        with_dicts(Expr::new(ExprKind::Local(unique), span), dicts)
    }
}
