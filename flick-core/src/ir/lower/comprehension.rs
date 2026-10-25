//! `for` as the checker reads it (`types::comprehension`): the clauses nest,
//! each binding an `Each` over its source, each guard an `If`, each `let` a
//! block, around the `Yield` of the element.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Pat};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::comprehension;

impl Lower<'_> {
    /// `[a, b]`, or the strict list of a lone `[for …]`.
    pub(super) fn list(&mut self, node: &Node, items: &[&Node]) -> Expr {
        if let Some(strict) = comprehension(node) {
            return self.for_expr(strict, true);
        }
        let items = items.iter().map(|c| self.expr(c)).collect();
        Expr::new(ExprKind::List(items), self.span(node))
    }

    pub(super) fn for_expr(&mut self, node: &Node, strict: bool) -> Expr {
        let span = self.span(node);
        let clauses: Vec<&Node> = node
            .nodes()
            .filter(|n| n.kind == K::Block)
            .flat_map(Node::nodes)
            .collect();
        let element = node.nodes().filter(|n| n.kind != K::Block).last();
        let mark = self.scope.len();
        let body = self.clauses(&clauses, element, span);
        self.scope.truncate(mark);
        let body = Box::new(body);
        Expr::new(ExprKind::Generate { body, strict }, span)
    }

    fn clauses(&mut self, clauses: &[&Node], element: Option<&Node>, span: Span) -> Expr {
        let unit = |span| Expr::new(ExprKind::Lit(Lit::Unit), span);
        let Some((&first, rest)) = clauses.split_first() else {
            let value = element.map_or_else(|| unit(span), |e| self.expr(e));
            return Expr::new(ExprKind::Yield(Box::new(value)), span);
        };
        let at = self.span(first);
        let kind = match first.kind {
            K::BindStmt => {
                let mut children = first.nodes();
                let (pattern, source) = (children.next(), children.next());
                // the source cannot see the names its pattern binds
                let source = source.map_or_else(|| unit(at), |s| self.expr(s));
                let pat = pattern.map_or(Pat::Wild, |p| self.pattern(p));
                let body = self.clauses(rest, element, span);
                ExprKind::Each {
                    pat,
                    source: Box::new(source),
                    body: Box::new(body),
                }
            }
            K::GuardStmt => {
                let condition = first
                    .nodes()
                    .next()
                    .map_or_else(|| unit(at), |c| self.expr(c));
                let body = self.clauses(rest, element, span);
                ExprKind::If(Box::new(condition), Box::new(body), Box::new(unit(at)))
            }
            K::LetDecl => {
                let stmt = self.local_let(first);
                let body = self.clauses(rest, element, span);
                ExprKind::Block(vec![stmt], Box::new(body))
            }
            _ => return self.clauses(rest, element, span),
        };
        Expr::new(kind, at)
    }
}
