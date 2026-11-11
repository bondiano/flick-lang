//! `for` as the checker reads it (`types::comprehension`): the clauses nest,
//! each binding an `Each` over its source, each guard an `If`, each `let` a
//! block, around the `Yield` of the element.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Pat};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{comprehension, for_parts, range_list};

impl Lower<'_> {
    /// `[a, b]`, or the strict list of a lone `[for …]`.
    pub(super) fn list(&mut self, node: &Node, items: &[&Node]) -> Expr {
        if let Some(strict) = comprehension(node) {
            return self.for_expr(strict, true);
        }
        if let Some(range) = range_list(node) {
            return self.range(range, true);
        }
        let items = items.iter().map(|c| self.expr(c)).collect();
        Expr::new(ExprKind::List(items), self.span(node))
    }

    pub(super) fn for_expr(&mut self, node: &Node, strict: bool) -> Expr {
        let span = self.span(node);
        let (clauses, element) = for_parts(node);
        let clauses: Vec<&Node> = clauses.collect();
        let looping = node.has_token(K::DoKw);
        let mark = self.scope.len();
        let control = looping.then(|| self.start_loop());
        let mut body = self.clauses(&clauses, element, looping, span);
        if let Some((id, _)) = control {
            self.loops.pop();
            body = Expr::new(
                ExprKind::ControlLoop {
                    id,
                    condition: None,
                    body: Box::new(body),
                    iterable: true,
                },
                span,
            );
        }
        self.scope.truncate(mark);
        // the loop of `for … do` runs here, outside any `Generate`
        if looping {
            return body;
        }
        let body = Box::new(body);
        let sync = self.typing.sync_generators.contains(&node.range());
        let limit = None;
        Expr::new(
            ExprKind::Generate {
                body,
                strict,
                sync,
                limit,
            },
            span,
        )
    }

    fn clauses(
        &mut self,
        clauses: &[&Node],
        element: Option<&Node>,
        looping: bool,
        span: Span,
    ) -> Expr {
        let unit = |span| Expr::new(ExprKind::Lit(Lit::Unit), span);
        let Some((&first, rest)) = clauses.split_first() else {
            let value = element.map_or_else(|| unit(span), |e| self.expr(e));
            if looping {
                return value;
            }
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
                let body = self.clauses(rest, element, looping, span);
                ExprKind::Each {
                    stop: None,
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
                let body = self.clauses(rest, element, looping, span);
                ExprKind::If(Box::new(condition), Box::new(body), Box::new(unit(at)))
            }
            K::LetDecl => {
                let stmt = self.local_let(first);
                let body = self.clauses(rest, element, looping, span);
                ExprKind::Block(vec![stmt], Box::new(body))
            }
            _ => return self.clauses(rest, element, looping, span),
        };
        Expr::new(kind, at)
    }
}
