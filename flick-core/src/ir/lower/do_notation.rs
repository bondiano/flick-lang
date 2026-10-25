//! `do { … }` as the checker reads it (`types::do_notation`): each binding
//! and expression statement is `flat_map (fun p -> rest) value` through the
//! `Monad` dictionary recorded at its key, a guard is `if c then rest else
//! nothing`.

use super::Lower;
use super::expr::with_dicts;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stmt};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Range, bind_key, guard_key};

impl Lower<'_> {
    pub(super) fn do_expr(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let statements: Vec<&Node> = node
            .nodes()
            .filter(|n| n.kind == K::Block)
            .flat_map(Node::nodes)
            .collect();
        let mark = self.scope.len();
        let expr = self.do_rest(&statements, span);
        self.scope.truncate(mark);
        expr
    }

    fn do_rest(&mut self, statements: &[&Node], span: Span) -> Expr {
        let Some((&first, rest)) = statements.split_first() else {
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        match first.kind {
            K::Signature => self.do_rest(rest, span),
            K::LetDecl if first.has_token(K::AssertKw) => {
                self.let_assert(first, span, |l| l.do_rest(rest, span))
            }
            K::LetDecl => {
                let stmt: Stmt = self.local_let(first);
                let body = self.do_rest(rest, span);
                Expr::new(ExprKind::Block(vec![stmt], Box::new(body)), span)
            }
            K::GuardStmt => {
                let condition = match first.nodes().next() {
                    Some(c) => self.expr(c),
                    None => Expr::new(ExprKind::Lit(Lit::Bool(true)), span),
                };
                let then = self.do_rest(rest, span);
                let nothing = self.prelude_method(guard_key(first), "nothing");
                Expr::new(
                    ExprKind::If(Box::new(condition), Box::new(then), Box::new(nothing)),
                    span,
                )
            }
            _ if rest.is_empty() => self.expr(first),
            _ => {
                let mut children = first.nodes();
                let (pattern, value) = if first.kind == K::BindStmt {
                    (children.next(), children.next())
                } else {
                    (None, Some(first))
                };
                let at = self.span(first);
                let flat_map = self.prelude_method(bind_key(first), "flat_map");
                // the value cannot see the names its pattern binds
                let value = match value {
                    Some(v) => self.expr(v),
                    None => Expr::new(ExprKind::Lit(Lit::Unit), at),
                };
                let mark = self.scope.len();
                let pat = pattern.map_or(Pat::Wild, |p| self.pattern(p));
                let body = self.do_rest(rest, span);
                self.scope.truncate(mark);
                let continuation = Expr::new(
                    ExprKind::Lambda {
                        params: vec![Param {
                            pat,
                            runtime: false,
                        }],
                        body: Box::new(body),
                    },
                    at,
                );
                let app = |func, arg| ExprKind::App {
                    func: Box::new(func),
                    arg: Box::new(arg),
                    runtime: false,
                    arg_first: false,
                };
                let partial = Expr::new(app(flat_map, continuation), at);
                Expr::new(app(partial, value), at)
            }
        }
    }

    /// The method `name` through the dictionary the checker recorded at `key`.
    fn prelude_method(&mut self, key: Range, name: &str) -> Expr {
        let span = Span::new(self.file, key.0, key.1);
        let mut dicts = self.typing.dicts.get(&key).cloned().unwrap_or_default();
        assert!(
            !dicts.is_empty(),
            "the checker resolved the `{name}` of a `do` block"
        );
        let dict = dicts.remove(0);
        let method = ExprKind::Method {
            dict,
            name: name.to_owned(),
        };
        with_dicts(Expr::new(method, span), dicts)
    }
}
