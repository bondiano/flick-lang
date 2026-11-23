//! Lowering of expression holes (`f a _ c`) to lambdas (`fun $hole -> f a $hole c`).

use flick_ir::IrType;

use super::Lower;
use super::expr::with_dicts;
use super::types::{apply_lift_type, lambda_type};
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
        let body = self.apply_inner(children[0], &children[1..], false);
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
        let ty = self.ir_type(hole.range());
        let binder_types = ty
            .clone()
            .map(|t| vec![(param_name.clone(), t)])
            .unwrap_or_default();
        let params = vec![Param {
            pat: Pat::Bind(param_name),
            stage,
            ty,
            binder_types,
        }];
        let lam_ty = self.ir_type(key).or_else(|| lambda_type(&params, &body));
        Expr::with_ty(
            ExprKind::Lambda {
                params,
                body: Box::new(body),
            },
            span,
            lam_ty,
        )
    }

    fn adapt_hole_body(&self, key: Range, mut value: Expr) -> Expr {
        if let Some(lift) = self.typing.hole_body_lifts.get(&key).cloned() {
            let span = value.span;
            let lifted = value.ty.as_ref().map(|t| apply_lift_type(t, &lift));
            value = Expr::with_ty(ExprKind::Lift(Box::new(value), lift), span, lifted);
        }
        if let Some(coercion) = self.typing.hole_body_coercions.get(&key).cloned() {
            let span = value.span;
            let target = match self.ir_type(key).as_deref() {
                Some(IrType::Fun { result, .. }) => Some(result.clone()),
                _ => None,
            };
            value = Self::coercion_expr(value, coercion.dict, coercion.method, span, target);
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
        let ty = self.ir_type(node.range());
        with_dicts(Expr::with_ty(ExprKind::Local(unique), span, ty), dicts)
    }
}
