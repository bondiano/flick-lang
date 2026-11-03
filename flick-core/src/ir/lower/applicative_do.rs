//! A `do` group `p1 <- e1 and p2 <- e2 …` as the checker reads it
//! (`types::applicative_do`): `map2` at each `and` folds the values into
//! their tuple; the last computes `e` of a following `pure e`, or the rest
//! of the block is `flat_map` over the tuple.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{and_keys, group_key, pure_argument};

impl Lower<'_> {
    pub(super) fn do_group(&mut self, group: &Node, rest: &[&Node], span: Span) -> Expr {
        let binds: Vec<(Option<&Node>, Option<&Node>)> = group
            .nodes()
            .filter(|n| n.kind == K::BindStmt)
            .map(|b| {
                let mut children = b.nodes();
                (children.next(), children.next())
            })
            .collect();
        // the values see none of the names the group binds
        let values: Vec<Expr> = binds
            .iter()
            .map(|(_, v)| match v {
                Some(v) => self.expr(v),
                None => Expr::new(ExprKind::Lit(Lit::Unit), span),
            })
            .collect();
        let applicative = self.typing.applicative.contains(&group.range());
        let keys = and_keys(group);
        let mut values = values.into_iter();
        let Some(mut acc) = values.next() else {
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        // the tuple so far, by hidden names
        let mut held: Vec<String> = vec![self.hidden("$and")];
        for (i, (key, value)) in keys.into_iter().zip(values).enumerate() {
            let at = Span::new(self.file, key.0, key.1);
            let map2 = self.prelude_method(key, Lang::Map2);
            let last = i + 2 == binds.len();
            let earlier = if i == 0 {
                Pat::Bind(held[0].clone())
            } else {
                Pat::Tuple(held.iter().cloned().map(Pat::Bind).collect())
            };
            let function = if last && applicative {
                // the patterns themselves, around `e` of `pure e`
                let mark = self.scope.len();
                let patterns: Vec<Pat> = binds.iter().map(|(p, _)| self.bind_pattern(*p)).collect();
                let (init, [newest]) = patterns.split_at(patterns.len() - 1) else {
                    unreachable!("a group has two bindings at least")
                };
                let first = if init.len() == 1 {
                    init[0].clone()
                } else {
                    Pat::Tuple(init.to_vec())
                };
                let body = rest.first().and_then(|s| pure_argument(s)).map_or_else(
                    || Expr::new(ExprKind::Lit(Lit::Unit), span),
                    |e| self.expr(e),
                );
                self.scope.truncate(mark);
                lambda(vec![first, newest.clone()], body, at)
            } else {
                let newest = self.hidden("$and");
                held.push(newest.clone());
                let tuple = held
                    .iter()
                    .map(|n| Expr::new(ExprKind::Local(n.clone()), at))
                    .collect();
                lambda(
                    vec![earlier, Pat::Bind(newest)],
                    Expr::new(ExprKind::Tuple(tuple), at),
                    at,
                )
            };
            acc = apply(apply(apply(map2, function, at), acc, at), value, at);
        }
        if applicative {
            return acc;
        }
        let key = group_key(group);
        let at = Span::new(self.file, key.0, key.1);
        let flat_map = self.prelude_method(key, Lang::FlatMap);
        let mark = self.scope.len();
        let patterns: Vec<Pat> = binds.iter().map(|(p, _)| self.bind_pattern(*p)).collect();
        let body = self.do_rest(rest, span);
        self.scope.truncate(mark);
        let continuation = lambda(vec![Pat::Tuple(patterns)], body, at);
        apply(apply(flat_map, continuation, at), acc, at)
    }

    fn bind_pattern(&mut self, pattern: Option<&Node>) -> Pat {
        pattern.map_or(Pat::Wild, |p| self.pattern(p))
    }
}

fn lambda(params: Vec<Pat>, body: Expr, span: Span) -> Expr {
    let params = params
        .into_iter()
        .map(|pat| Param {
            pat,
            stage: Stage::Pure,
            ty: None,
        })
        .collect();
    Expr::new(
        ExprKind::Lambda {
            params,
            body: Box::new(body),
        },
        span,
    )
}

fn apply(func: Expr, arg: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        },
        span,
    )
}
