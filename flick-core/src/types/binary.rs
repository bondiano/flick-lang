//! Binary operators. A chain nested to the left, as `a + b + c` or
//! `x |> f |> g` parse, is typed down its left side in a loop: recursion
//! takes stack per operator, and a long chain overflowed a thread's stack.

use super::ty::Type;
use super::{Checker, Domain, Operator};
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

/// The operator and the operands of `node` when it is a binary operator
/// that types its left operand first, with `infer` and nothing before it.
fn left_first(node: &Node) -> Option<(Token, &Node, &Node)> {
    if node.kind != K::BinExpr {
        return None;
    }
    let op = node.child_tokens().find(|t| t.kind.is_binary_op())?;
    if matches!(op.kind, K::LtPipe | K::GtGt | K::LtLt | K::OrOr | K::AndAnd) {
        return None;
    }
    let mut operands = node.nodes();
    Some((op, operands.next()?, operands.next()?))
}

impl Checker<'_> {
    pub fn binary(&mut self, node: &Node, children: &[&Node]) -> Type {
        let Some(op) = node.child_tokens().find(|t| t.kind.is_binary_op()) else {
            return Type::Error;
        };
        let (Some(lhs), Some(rhs)) = (children.first(), children.get(1)) else {
            return Type::Error;
        };
        let bool_ = Type::lang(Lang::Bool);
        match op.kind {
            K::LtPipe => {
                let ty = self.right_apply(lhs, rhs);
                self.note_type(self.token_span(op), &ty);
                ty
            }
            K::GtGt | K::LtLt => self.compose(node, None),
            K::OrOr | K::AndAnd => {
                self.check(lhs, &bool_);
                self.check(rhs, &bool_);
                bool_
            }
            _ => {
                let ty = self.infer_left(lhs);
                self.binary_after_left(node, op, lhs, rhs, ty)
            }
        }
    }

    /// `infer` of the left operand of a binary operator: the operators of a
    /// chain below it are typed bottom up, as the recursion would, each
    /// noted as `infer` notes it.
    fn infer_left(&mut self, lhs: &Node) -> Type {
        let mut spine = Vec::new();
        let mut node = lhs;
        while let Some((op, left, right)) = left_first(node) {
            spine.push((node, op, left, right));
            node = left;
        }
        let mut ty = self.infer(node);
        for (node, op, left, right) in spine.into_iter().rev() {
            ty = self.binary_after_left(node, op, left, right, ty);
            self.note_head(node, &ty);
        }
        ty
    }

    /// What a binary operator that [`left_first`] names does once its left
    /// operand has type `ty`.
    fn binary_after_left(
        &mut self,
        node: &Node,
        op: Token,
        lhs: &Node,
        rhs: &Node,
        ty: Type,
    ) -> Type {
        let span = self.span(node);
        let bool_ = Type::lang(Lang::Bool);
        match op.kind {
            K::PipeGt => {
                // at the `|>`, the type after the stage: hover and inlay hints show it
                let ty = self.pipe_from(ty, lhs, rhs);
                self.note_type(self.token_span(op), &ty);
                ty
            }
            K::EqEq | K::NotEq => {
                self.check(rhs, &ty);
                let givens = self.equalities_in_scope();
                self.equalities.push((ty, span, givens));
                bool_
            }
            kind => {
                self.check(rhs, &ty);
                let (name, domain) = match kind {
                    K::Lt => ("<", Domain::Ordered),
                    K::Le => ("<=", Domain::Ordered),
                    K::Gt => (">", Domain::Ordered),
                    K::Ge => (">=", Domain::Ordered),
                    K::PlusPlus => ("++", Domain::Concat),
                    K::Plus => ("+", Domain::Numeric),
                    K::Minus => ("-", Domain::Numeric),
                    K::Star => ("*", Domain::Numeric),
                    K::Slash => ("/", Domain::Numeric),
                    K::Percent => ("%", Domain::Numeric),
                    _ => ("**", Domain::Numeric),
                };
                self.cx.operators.push(Operator {
                    ty: ty.clone(),
                    span,
                    op: name,
                    domain,
                    givens: self.classes.givens.clone(),
                });
                if domain == Domain::Ordered { bool_ } else { ty }
            }
        }
    }
}
