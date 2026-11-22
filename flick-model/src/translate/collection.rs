//! Strict list comprehensions and lazy `Seq` comprehensions.
//! Both traverse any supported `Iterable` source (`List` or `Seq`) via `SeqIter`
//! and `SeqNext`; each iteration keeps its local SSA bindings, while outer
//! assignments are rejected.
use std::rc::Rc;

use super::{
    Tr,
    expr::{cond, pat, stuck},
    loops::{seq_iter, seq_step},
};
use crate::syntax::{Arm, Const, Pat, Path, Prim, Term};
use flick_core::ir::{self, ExprKind, Stmt};

/// Yield becomes a singleton list; a nested clause starts another collection.
fn collected(e: &ir::Expr) -> ir::Expr {
    let kind = match &e.kind {
        ExprKind::Yield(value) => ExprKind::List(vec![(**value).clone()]),
        ExprKind::Each { .. } => ExprKind::Generate {
            body: Box::new(e.clone()),
            strict: true,
            sync: false,
            limit: None,
        },
        ExprKind::If(c, yes, no) => {
            ExprKind::If(c.clone(), Box::new(collected(yes)), Box::new(collected(no)))
        }
        ExprKind::Block(stmts, tail) => ExprKind::Block(stmts.clone(), Box::new(collected(tail))),
        ExprKind::Lit(ir::Lit::Unit) => ExprKind::List(Vec::new()),
        _ => return e.clone(),
    };
    ir::Expr::new(kind, e.span)
}

impl Tr<'_> {
    pub(super) fn generate(
        &mut self,
        body: &ir::Expr,
        strict: bool,
        sync: bool,
        limit: Option<&ir::Expr>,
    ) -> Term {
        if strict {
            match limit {
                None => self.strict_collection(body),
                Some(limit) => Term::app(
                    Term::Global(Path::new("Seq", "to_list")),
                    self.lazy_collection(body, sync, Some(limit)),
                ),
            }
        } else {
            self.lazy_collection(body, sync, limit)
        }
    }
    pub(super) fn strict_collection(&mut self, e: &ir::Expr) -> Term {
        let ExprKind::Each {
            stop,
            pat: p,
            source,
            body,
            ..
        } = &e.kind
        else {
            return self.expr(&collected(e));
        };
        if stop.is_some() {
            return stuck("a `for` comprehension with a stop predicate");
        }
        let source = seq_iter(self.expr(source));
        let name = self.fresh("collect");
        let seq = self.fresh("seq");
        let head = self.fresh("head");
        let tail = self.fresh("tail");
        let task = self.iteration_effectful(body);
        let call = |term| {
            if task {
                Term::Await(Rc::new(term))
            } else {
                term
            }
        };
        let next = call(Term::app(Term::Var(name.clone()), Term::Var(tail.clone())));
        let values = self.strict_collection(body);
        let append = Term::apps(Term::prim(Prim::ListConcat), [values, next.clone()]);
        let matched = Term::Match(
            Rc::new(Term::Var(head.clone())),
            Rc::from([
                Arm {
                    pat: pat(p),
                    guard: None,
                    body: append,
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: next,
                },
            ]),
        );
        let step = seq_step(&seq, head, tail, Term::List(Rc::from([])), matched);
        let step = if task {
            Term::Task(Rc::new(step))
        } else {
            step
        };
        let function = Term::lam(&seq, step);
        Term::LetRec(
            name.clone(),
            None,
            Rc::new(function),
            Rc::new(call(Term::app(Term::Var(name), source))),
        )
    }

    pub(super) fn lazy_collection(
        &mut self,
        body: &ir::Expr,
        sync: bool,
        limit: Option<&ir::Expr>,
    ) -> Term {
        let inner = self.lazy_clause(body, sync);
        let seq = if sync {
            Term::app(Term::konst(Const::SeqSync), Term::lam("_", inner))
        } else {
            Term::app(
                Term::konst(Const::SeqThunk),
                Term::lam("_", Term::app(Term::konst(Const::SeqNext), inner)),
            )
        };
        match limit {
            Some(limit) => Term::apps(
                Term::Global(Path::new("Seq", "take")),
                [self.expr(limit), seq],
            ),
            None => seq,
        }
    }

    fn lazy_clause(&mut self, e: &ir::Expr, sync: bool) -> Term {
        match &e.kind {
            ExprKind::Yield(value) => Term::app(
                Term::Global(Path::new("Seq", "singleton")),
                self.expr(value),
            ),
            ExprKind::Lit(ir::Lit::Unit) => Term::Global(Path::new("Seq", "empty")),
            ExprKind::If(c, yes, no) => cond(
                self.expr(c),
                self.lazy_clause(yes, sync),
                self.lazy_clause(no, sync),
            ),
            ExprKind::Block(stmts, tail) => self.lazy_block(stmts, tail, sync),
            ExprKind::Each { stop: Some(_), .. } => {
                stuck("a `for` comprehension with a stop predicate")
            }
            ExprKind::Each {
                stop: None,
                pat: p,
                source,
                body,
                ..
            } => {
                let source = seq_iter(self.expr(source));
                let item = self.fresh("item");
                let inner = self.lazy_clause(body, sync);
                let matched = Term::Match(
                    Rc::new(Term::Var(item.clone())),
                    Rc::from([
                        Arm {
                            pat: pat(p),
                            guard: None,
                            body: inner,
                        },
                        Arm {
                            pat: Pat::Wild,
                            guard: None,
                            body: Term::Global(Path::new("Seq", "empty")),
                        },
                    ]),
                );
                let mapper = Term::lam(&item, matched);
                let flat_map = if sync {
                    Term::konst(Const::SeqFlatMapSync)
                } else {
                    Term::Global(Path::new("Seq", "flat_map"))
                };
                Term::apps(flat_map, [mapper, source])
            }
            _ => stuck("an unsupported `for` comprehension body"),
        }
    }

    fn lazy_block(&mut self, stmts: &[Stmt], tail: &ir::Expr, sync: bool) -> Term {
        let Some((first, rest)) = stmts.split_first() else {
            return self.lazy_clause(tail, sync);
        };
        let rest = self.lazy_block(rest, tail, sync);
        self.stmt(first, rest)
    }
}

#[cfg(test)]
mod tests {
    use flick_core::ir::{Expr, ExprKind, Lit, Pat};
    use flick_core::source::{FileId, Span};

    use super::Tr;
    use crate::check::Checker;
    use crate::decls::Decls;

    #[test]
    fn for_with_stop_predicate_reports_translation_gap() {
        let span = Span::new(FileId::new(0), 0, 0);
        let stop_each = |strict| {
            Expr::new(
                ExprKind::Generate {
                    body: Box::new(Expr::new(
                        ExprKind::Each {
                            stop: Some(Box::new(Expr::new(ExprKind::Lit(Lit::Bool(true)), span))),
                            pat: Pat::Bind("x".into()),
                            binder_types: Vec::new(),
                            source: Box::new(Expr::new(ExprKind::List(Vec::new()), span)),
                            body: Box::new(Expr::new(
                                ExprKind::Yield(Box::new(Expr::new(
                                    ExprKind::Local("x".into()),
                                    span,
                                ))),
                                span,
                            )),
                        },
                        span,
                    )),
                    strict,
                    sync: false,
                    limit: None,
                },
                span,
            )
        };
        let decls = Decls::default();
        for strict in [true, false] {
            let translated = Tr::new("T").expr(&stop_each(strict));
            let err = Checker::new(&decls)
                .infer_top(&translated)
                .expect_err("stop predicate must report a translation gap");
            assert_eq!(err.rule, "Translation", "{err}");
        }
    }
}
