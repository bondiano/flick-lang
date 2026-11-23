//! The selected Eff worker in calls staged for owner evidence.

use flick_core::ir::{self, ExprKind, Stmt};
use flick_core::types::lang::Lang;

use super::{Tr, is_lang};
use crate::syntax::Term;

impl Tr<'_> {
    /// An owner-evidence bracket stages the callable and its argument outside
    /// the bracket. Recover the known worker and keep the task evaluation at
    /// its original position.
    pub(super) fn owner_await(&mut self, stmts: &[Stmt], tail: &ir::Expr) -> Option<Term> {
        let (first, operands) = stmts.split_first()?;
        let Stmt::Let {
            pat: ir::Pat::Bind(worker),
            value,
            ..
        } = first
        else {
            return None;
        };
        let ExprKind::WithOwnerEvidence(owner, call) = &tail.kind else {
            return None;
        };
        let ExprKind::App { func, arg, .. } = &call.kind else {
            return None;
        };
        if !is_lang(value, Lang::AwaitEff)
            || !matches!(&func.kind, ExprKind::Local(local) if local == worker)
        {
            return None;
        }
        let await_ = ir::Expr::with_ty(ExprKind::Await(arg.clone()), call.span, call.ty.clone());
        let bracket = ir::Expr::with_ty(
            ExprKind::WithOwnerEvidence(owner.clone(), Box::new(await_)),
            tail.span,
            tail.ty.clone(),
        );
        Some(self.block(operands, &bracket, None))
    }
}
