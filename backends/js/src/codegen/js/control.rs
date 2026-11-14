//! Local loop control, lowered through the shared IR loop contract.
use super::{
    Gen,
    doc::{After, Before, Js, MULTILINE, Stmt, block, stmt_flags},
};
use crate::codegen::loops;
use crate::ir::{Expr, ExprKind};
use flick_pretty::docvec;
impl<'d> Gen<'_, 'd> {
    pub(super) fn loop_expr(&mut self, e: &Expr) -> Js<'d> {
        match &e.kind {
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable,
            } => {
                let lowered = loops::lower(
                    self.options.lang,
                    id,
                    condition.as_deref(),
                    body,
                    *iterable,
                    e.span,
                );
                self.expr(&lowered)
            }
            ExprKind::LoopExit { .. } => unreachable!("loop exits are lowered with their loop"),
            ExprKind::Loop { condition, body } => {
                let a = self.a;
                let (stmts, ()) = self.block(|g| {
                    let condition = g.expr(condition);
                    g.buf.push(Stmt::new(
                        docvec![
                            a,
                            "if (!(",
                            condition.at(a, Before::Delimiter, After::Delimiter),
                            ")) break;"
                        ],
                        condition.flags,
                    ));
                    g.discard(body);
                });
                self.buf.push(Stmt::new(
                    docvec![a, "while (true) ", block(a, &stmts)],
                    stmt_flags(&stmts) | MULTILINE,
                ));
                self.leaf("undefined")
            }
            _ => unreachable!("loop expression"),
        }
    }
}
