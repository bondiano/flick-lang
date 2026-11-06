//! Ranges as the checker reads them (`types::ranges`): calls of `Seq`.

use super::Lower;
use crate::ir::ExprLang;
use crate::ir::{Expr, ExprKind, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

impl Lower<'_> {
    /// `Seq.range_to a b`, `Seq.range a b` or `Seq.from a`; with `strict`,
    /// the `List` of it.
    pub(super) fn range(&mut self, node: &Node, strict: bool) -> Expr {
        let span = self.span(node);
        let bounds: Vec<Expr> = node.nodes().map(|b| self.expr(b)).collect();
        let function = match (bounds.len(), node.has_token(K::DotDotLt)) {
            (1, _) => Lang::SeqFrom,
            (_, true) => Lang::SeqRange,
            _ => Lang::SeqRangeTo,
        };
        let range = bounds
            .into_iter()
            .fold(seq(function, span), |f, bound| apply(f, bound, span));
        if strict {
            apply(seq(Lang::SeqToList, span), range, span)
        } else {
            range
        }
    }
}

fn seq(function: Lang, span: Span) -> Expr {
    Expr::new(ExprKind::lang(function), span)
}

fn apply(func: Expr, arg: Expr, span: Span) -> Expr {
    let app = ExprKind::App {
        func: Box::new(func),
        arg: Box::new(arg),
        stage: Stage::Pure,
        arg_first: false,
    };
    Expr::new(app, span)
}
