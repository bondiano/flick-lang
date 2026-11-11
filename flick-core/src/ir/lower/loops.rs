//! Preserve lexical loop exits in the shared IR; backends lower them separately.
use super::Lower;
use crate::ir::{Expr, ExprKind, Lit};
use crate::syntax::{cst::Node, kind::SyntaxKind as K};
impl Lower<'_> {
    pub(super) fn start_loop(&mut self) -> (String, String) {
        let names = (self.hidden("$loop"), self.hidden("$result"));
        self.loops.push(names.clone());
        names
    }
    pub(super) fn control_loop(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let (id, _) = self.start_loop();
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
        let id = self.loops.last().expect("checked loop exit").0.clone();
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
