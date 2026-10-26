//! `fun` and `component`: a function of its parameters, which a component
//! gives to `Ui.view`.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Stage};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_pattern;

impl Lower<'_> {
    /// `fun params -> body`, and the props and render of a component.
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

    /// `Ui.view (fun props -> render)`; the checker refused a `setup`.
    pub(super) fn component(&mut self, node: &Node, children: &[&Node]) -> Expr {
        let span = self.span(node);
        let render = self.lambda(node, children);
        let view = ExprKind::Global {
            module: "Ui".to_owned(),
            name: "view".to_owned(),
        };
        let app = ExprKind::App {
            func: Box::new(Expr::new(view, span)),
            arg: Box::new(render),
            stage: Stage::Pure,
            arg_first: false,
        };
        Expr::new(app, span)
    }
}
