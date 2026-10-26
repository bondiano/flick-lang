//! Ranges as the checker reads them (`types::ranges`): calls of `Seq`.

use super::Lower;
use crate::ir::{Expr, ExprKind, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Lower<'_> {
    /// `Seq.range_to a b`, `Seq.range a b` or `Seq.from a`; with `strict`,
    /// the `List` of it.
    pub(super) fn range(&mut self, node: &Node, strict: bool) -> Expr {
        let span = self.span(node);
        let bounds: Vec<Expr> = node.nodes().map(|b| self.expr(b)).collect();
        let function = match (bounds.len(), node.has_token(K::DotDotLt)) {
            (1, _) => "from",
            (_, true) => "range",
            _ => "range_to",
        };
        let range = bounds
            .into_iter()
            .fold(seq(function, span), |f, bound| apply(f, bound, span));
        if strict {
            apply(seq("to_list", span), range, span)
        } else {
            range
        }
    }
}

fn seq(name: &str, span: Span) -> Expr {
    let global = ExprKind::Global {
        module: "Seq".to_owned(),
        name: name.to_owned(),
    };
    Expr::new(global, span)
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
