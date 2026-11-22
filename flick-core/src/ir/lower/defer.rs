//! Lowering of scope-owned `defer` and `errdefer` statements onto `Scope.defer`
//! with explicit lexical owner evidence.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Lower<'_> {
    /// Lowers `defer { cleanup }` / `errdefer { cleanup }` onto `Scope.defer`
    /// with explicit lexical owner evidence for the selected enclosing `scope`.
    pub(super) fn defer_stmt(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let is_errdefer = node.has_token(K::ErrdeferKw);
        let body = match node.nodes().next() {
            Some(block) => self.expr(block),
            None => Expr::new(ExprKind::Lit(Lit::Unit), span),
        };
        let callback = if is_errdefer {
            let exit = self.hidden("$scope_exit");
            let check = Expr::new(
                ExprKind::App {
                    func: Box::new(Expr::new(
                        ExprKind::Global {
                            module: "Exit".to_owned(),
                            name: "is_failure".to_owned(),
                            type_args: Vec::new(),
                        },
                        span,
                    )),
                    arg: Box::new(Expr::new(ExprKind::Local(exit.clone()), span)),
                    stage: Stage::Pure,
                    arg_first: false,
                },
                span,
            );
            let guarded = Expr::new(
                ExprKind::If(
                    Box::new(check),
                    Box::new(body),
                    Box::new(Expr::new(ExprKind::Lit(Lit::Unit), span)),
                ),
                span,
            );
            Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Bind(exit),
                        stage: Stage::Runtime,
                        ty: None,
                        binder_types: Vec::new(),
                    }],
                    body: Box::new(guarded),
                },
                span,
            )
        } else {
            Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Wild,
                        stage: Stage::Runtime,
                        ty: None,
                        binder_types: Vec::new(),
                    }],
                    body: Box::new(body),
                },
                span,
            )
        };
        let call = Expr::new(
            ExprKind::App {
                func: Box::new(Expr::new(
                    ExprKind::Global {
                        module: "Scope".to_owned(),
                        name: "defer".to_owned(),
                        type_args: Vec::new(),
                    },
                    span,
                )),
                arg: Box::new(callback),
                stage: Stage::Runtime,
                arg_first: false,
            },
            span,
        );
        self.within(node.range(), call)
    }
}
