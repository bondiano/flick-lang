//! `fun` and `component`: a function of its parameters, which a component
//! gives to `Ui.view`, or to `Ui.stateful` with its setup.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Stage, Stmt};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{component_parts, is_pattern};

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

    /// `Ui.view (fun props -> render)`, or with a setup `Ui.stateful (fun
    /// props -> setup; fun props -> render)`.
    pub(super) fn component(&mut self, node: &Node, children: &[&Node]) -> Expr {
        let span = self.span(node);
        let parts = component_parts(node);
        let (Some(setup), Some(render)) = (parts.setup, parts.render) else {
            let render = self.lambda(node, children);
            return Self::ui_call("view", render, span);
        };
        let props: Vec<&Node> = parts.props.into_iter().collect();
        let stages = |l: &Self, n: &Node| l.typing.stages.get(&n.range()).cloned();
        let (outer, inner) = (stages(self, node), stages(self, render));
        let setup = self.function_with(
            &props,
            |l| {
                let statements: Vec<&Node> = setup.nodes().collect();
                let mut stmts = Vec::new();
                for statement in statements {
                    match statement.kind {
                        K::Signature => {}
                        K::LetDecl => stmts.push(l.local_let(statement)),
                        _ => stmts.push(Stmt::Expr(l.expr(statement))),
                    }
                }
                let render = l.function(&props, render, inner, l.span(render));
                if stmts.is_empty() {
                    return render;
                }
                Expr::new(ExprKind::Block(stmts, Box::new(render)), span)
            },
            outer,
            span,
        );
        Self::ui_call("stateful", setup, span)
    }

    /// `Ui.name f`.
    fn ui_call(name: &str, f: Expr, span: Span) -> Expr {
        let global = ExprKind::Global {
            module: "Ui".to_owned(),
            name: name.to_owned(),
        };
        let app = ExprKind::App {
            func: Box::new(Expr::new(global, span)),
            arg: Box::new(f),
            stage: Stage::Pure,
            arg_first: false,
        };
        Expr::new(app, span)
    }
}
