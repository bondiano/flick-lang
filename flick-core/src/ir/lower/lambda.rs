//! `fun`: a function of its parameters.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_pattern;

impl Lower<'_> {
    /// `fun params -> body`.
    pub(super) fn lambda(&mut self, node: &Node, children: &[&Node]) -> Expr {
        let span = self.span(node);
        let params: Vec<&Node> = children
            .iter()
            .copied()
            .filter(|n| is_pattern(n.kind))
            .collect();
        let Some(body) = children.iter().find(|n| n.kind == K::Block) else {
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let stages = self.typing.stages.get(&node.range()).cloned();
        self.function(&params, body, stages, span)
    }
}
