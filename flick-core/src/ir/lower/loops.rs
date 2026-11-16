//! Preserve lexical loop exits in the shared IR; backends lower them separately.
use super::Lower;
use crate::ir::{Expr, ExprKind, Lit};
use crate::syntax::{cst::Node, kind::SyntaxKind as K};

impl Lower<'_> {
    pub(super) fn loop_label(&self, node: &Node) -> Option<String> {
        self.token_text(node, K::Label)
            .map(|s| s.strip_prefix('\'').unwrap_or(&s).to_owned())
    }

    pub(super) fn start_loop(&mut self, label: Option<String>) -> String {
        let id = self.hidden("$loop");
        self.loops.push((label, id.clone()));
        id
    }

    pub(super) fn control_loop(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let label = self.loop_label(node);
        let id = self.start_loop(label);
        let mut parts = node.nodes();
        let condition = if node.kind == K::WhileExpr {
            Some(Box::new(self.expr(parts.next().expect("condition"))))
        } else {
            None
        };
        let body = Box::new(self.expr(parts.next().expect("body")));
        self.loops.pop();
        Expr::new(
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable: false,
            },
            span,
        )
    }

    pub(super) fn loop_exit(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let target = self.loop_label(node);
        let id = if let Some(ref target) = target {
            self.loops
                .iter()
                .rev()
                .find(|(l, _)| l.as_deref() == Some(target.as_str()))
                .map_or_else(
                    || self.loops.last().expect("checked loop exit").1.clone(),
                    |(_, id)| id.clone(),
                )
        } else {
            self.loops.last().expect("checked loop exit").1.clone()
        };
        let value = if node.kind == K::ContinueExpr {
            None
        } else {
            Some(Box::new(node.nodes().next().map_or_else(
                || Expr::new(ExprKind::Lit(Lit::Unit), span),
                |e| self.expr(e),
            )))
        };
        Expr::new(ExprKind::LoopExit { id, value }, span)
    }
}
