//! Postfix `?` as the checker read it (`types::question`).

use super::Lower;
use crate::ir::{Expr, ExprKind, Stage};
use crate::syntax::cst::Node;
use crate::types::Question;

impl Lower<'_> {
    /// `e?`: `from_result e` on a `Result`, an `Unwrap` on an `Option`.
    pub(super) fn question(&mut self, node: &Node, operand: &Node) -> Expr {
        let span = self.span(node);
        let operand = self.expr(operand);
        let kind = match self.typing.questions.get(&node.range()) {
            Some(Question::Option) => ExprKind::Unwrap(Box::new(operand)),
            Some(Question::Result) => ExprKind::App {
                func: Box::new(Expr::new(
                    ExprKind::Global {
                        module: "Prelude".to_owned(),
                        name: "from_result".to_owned(),
                    },
                    span,
                )),
                arg: Box::new(operand),
                stage: Stage::Runtime,
                arg_first: false,
            },
            None => return operand,
        };
        Expr::new(kind, span)
    }
}
