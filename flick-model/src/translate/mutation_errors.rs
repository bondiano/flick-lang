//! State and typed failure travel together while translating a protected body.
//! Caught errors become cold re-raise actions carried by Result until dispatch.
mod dispatch;
mod loops;

use super::{
    Tr,
    expr::{cond, is_lang, layer_run, pat},
    loops::{list_step, none, option, result, rpat, some, state_function, state_pattern},
};
use crate::syntax::{Arm, Name, Pat, Term};
use crate::types::{Scheme, Stage as ModelStage, Type};
use flick_core::ir::{self, ExprKind, Stmt};
use flick_core::types::lang::Lang;
use std::iter;
use std::rc::Rc;

impl Tr {
    pub(super) fn active_variables(&self) -> Vec<String> {
        let mut xs: Vec<_> = self.versions.iter().map(|(x, _)| x.clone()).collect();
        xs.sort();
        xs.dedup();
        xs
    }
    fn captured(&self, ok: bool, value: Term, xs: &[String]) -> Term {
        let value = if ok && self.control.is_some() {
            result(true, value)
        } else {
            value
        };
        self.pack(result(ok, value), xs)
    }
    pub(super) fn catch_atomic(&mut self, value: Term, xs: &[String], can_raise: bool) -> Term {
        let v = self.fresh("value");
        let body = Term::Let(
            v.clone(),
            None,
            Rc::new(value),
            Rc::new(self.captured(true, Term::Var(v), xs)),
        );
        self.catch_failure(body, xs, can_raise)
    }
    pub(super) fn catch_failure(&mut self, body: Term, xs: &[String], can_raise: bool) -> Term {
        if !can_raise {
            return body;
        }
        let error = self.fresh("error");
        let arm = Arm {
            pat: Pat::Var(error.clone()),
            guard: None,
            body: self.capture_error(Term::Var(error), xs),
        };
        Term::Try(Rc::new(body), Rc::from([arm]))
    }
    fn protected_with(
        &mut self,
        e: &ir::Expr,
        out: &[String],
        k: impl FnOnce(&mut Self, Term) -> Term,
    ) -> Term {
        if matches!(
            e.kind,
            ExprKind::Local(_) | ExprKind::Lambda { .. } | ExprKind::Lit(_)
        ) {
            let value = self.expr(e);
            return k(self, value);
        }
        let tracked = self.active_variables();
        let value = self.protected(e, &tracked);
        self.unpack_protected(value, &tracked, out, k)
    }
    pub(super) fn unpack_protected(
        &mut self,
        value: Term,
        tracked: &[String],
        out: &[String],
        k: impl FnOnce(&mut Self, Term) -> Term,
    ) -> Term {
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let v = self.fresh("value");
        let error = self.fresh("error");
        let good = k(self, Term::Var(v.clone()));
        let bad = self.captured(false, Term::Var(error.clone()), out);
        self.versions.truncate(mark);
        let pattern = |first| state_pattern(first, &names);
        let good_pat = if self.control.is_some() {
            rpat(true, rpat(true, Pat::Var(v)))
        } else {
            rpat(true, Pat::Var(v))
        };
        let mut arms = vec![
            Arm {
                pat: pattern(good_pat),
                guard: None,
                body: good,
            },
            Arm {
                pat: pattern(rpat(false, Pat::Var(error))),
                guard: None,
                body: bad,
            },
        ];
        if self.control.is_some() {
            let exit = self.fresh("exit");
            let mark = self.versions.len();
            self.versions
                .extend(tracked.iter().cloned().zip(names.iter().cloned()));
            let body = self.pack(result(true, result(false, Term::Var(exit.clone()))), out);
            self.versions.truncate(mark);
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, Pat::Var(exit)))),
                guard: None,
                body,
            });
        }
        Term::Match(Rc::new(value), arms.into())
    }

    fn protected_block(&mut self, stmts: &[Stmt], tail: &ir::Expr, xs: &[String]) -> Term {
        let Some((first, rest)) = stmts.split_first() else {
            return self.protected(tail, xs);
        };
        match first {
            Stmt::Assign(x, e) if self.cell(x).is_some() => self.protected_with(e, xs, |tr, v| {
                let rest = tr.protected_block(rest, tail, xs);
                tr.write_cell(x, v, rest)
            }),
            Stmt::Var(x, e) | Stmt::Assign(x, e) => self.protected_with(e, xs, |tr, v| {
                let name = tr.fresh(x);
                tr.versions.push((x.clone(), name.clone()));
                let body = tr.protected_block(rest, tail, xs);
                tr.versions.pop();
                Term::Let(name, None, Rc::new(v), Rc::new(body))
            }),
            Stmt::Let(p, e) => self.protected_with(e, xs, |tr, v| {
                let body = tr.protected_block(rest, tail, xs);
                Term::Match(
                    Rc::new(v),
                    Rc::from([Arm {
                        pat: pat(p),
                        guard: None,
                        body,
                    }]),
                )
            }),
            Stmt::Expr(e) => self.protected_with(e, xs, |tr, v| {
                let body = tr.protected_block(rest, tail, xs);
                Term::Let("_".into(), None, Rc::new(v), Rc::new(body))
            }),
            Stmt::Fun { .. } => {
                let body = self.protected_block(rest, tail, xs);
                self.stmt(first, body)
            }
        }
    }
    pub(super) fn protected(&mut self, e: &ir::Expr, xs: &[String]) -> Term {
        if let Some(id) = self.control.clone()
            && matches!(&e.kind, ExprKind::App { func, .. } if matches!(&func.kind, ExprKind::Local(n) if self.resumptions.iter().any(|(r, target)| r == n && target == &id)))
        {
            return self.protected_resumption(e, &id, xs);
        }
        match &e.kind {
            ExprKind::LoopExit { id, value } if self.control.as_deref() == Some(id.as_str()) => {
                match value {
                    Some(v) => self.protected_with(v, xs, |tr, v| {
                        tr.pack(result(true, result(false, option(Some(v)))), xs)
                    }),
                    None => self.pack(result(true, result(false, option(None))), xs),
                }
            }
            ExprKind::ControlLoop {
                id,
                body,
                iterable: true,
                ..
            } => self.protected_source_for(id, body, xs),
            ExprKind::Each {
                pat: p,
                source,
                body,
                ..
            } => self.protected_with(source, xs, |tr, v| tr.protected_each(p, v, body, xs)),
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable: false,
            } => self.protected_source_loop(id, condition.as_deref(), body, xs),
            ExprKind::Block(stmts, tail) => self.protected_block(stmts, tail, xs),
            ExprKind::If(c, a, b) => self.protected_with(c, xs, |tr, c| {
                let a = tr.protected(a, xs);
                let b = tr.protected(b, xs);
                cond(c, a, b)
            }),
            ExprKind::Binary(ir::BinOp::And | ir::BinOp::Or, a, b) => {
                self.protected_with(a, xs, |tr, c| {
                    let and = matches!(e.kind, ExprKind::Binary(ir::BinOp::And, _, _));
                    let rhs = tr.protected(b, xs);
                    let other = tr.captured(true, Term::bool(!and), xs);
                    if and {
                        cond(c, rhs, other)
                    } else {
                        cond(c, other, rhs)
                    }
                })
            }
            ExprKind::Match(s, arms) => {
                self.protected_with(s, xs, |tr, value| tr.protected_arms(value, arms, xs))
            }
            ExprKind::Handle(h) if h.is_try() => self.try_state_as(&h.body, &h.errors, xs, true),
            ExprKind::Handle(h) => self.protected_handler(h, xs),
            ExprKind::App { func, arg, .. } if is_lang(func, Lang::ScopeRun) => {
                self.scope_with(arg, &|tr, body| tr.protected(body, xs))
            }
            ExprKind::App { func, arg, .. } if layer_run(func).is_some() => {
                self.protected_provide(layer_run(func).expect("layer"), arg, xs)
            }
            ExprKind::Loop { condition, body } => self.protected_loop(condition, body, xs),
            ExprKind::Lambda { .. }
            | ExprKind::Abstract(..)
            | ExprKind::Task(_)
            | ExprKind::Generate { .. } => {
                let value = self.expr(e);
                self.catch_atomic(value, xs, false)
            }
            ExprKind::Dicts(inner, _) if matches!(inner.kind, ExprKind::Perform { .. }) => {
                let value = self.expr(e);
                self.catch_atomic(value, xs, false)
            }
            _ => {
                let mut children = Vec::new();
                e.each_child(&mut |c| children.push(c.clone()));
                self.protected_parts(e, &children, Vec::new(), xs)
            }
        }
    }
    fn protected_provide(&mut self, layer: &ir::Expr, arg: &ir::Expr, xs: &[String]) -> Term {
        self.protected_with(layer, xs, |tr, layer| {
            let value = tr.provide_value(layer, arg, &|tr, body| tr.protected(body, xs));
            // Acquisition can fail before the callback starts; its store is
            // the state after evaluating the layer argument.
            tr.catch_failure(value, xs, true)
        })
    }
    fn protected_parts(
        &mut self,
        e: &ir::Expr,
        parts: &[ir::Expr],
        mut names: Vec<String>,
        xs: &[String],
    ) -> Term {
        let Some((first, rest)) = parts.split_first() else {
            let mut rebuilt = e.clone();
            let mut names = names.into_iter();
            rebuilt.each_child_mut(&mut |c| {
                *c = ir::Expr::new(ExprKind::Local(names.next().expect("part")), c.span);
            });
            let value = self.expr(&rebuilt);
            return self.catch_atomic(
                value,
                xs,
                matches!(
                    e.kind,
                    ExprKind::App {
                        stage: ir::Stage::Runtime | ir::Stage::Sync,
                        ..
                    } | ExprKind::Await(_)
                        | ExprKind::Handle(_)
                ),
            );
        };
        self.protected_with(first, xs, |tr, value| {
            let name = tr.fresh("operand");
            names.push(name.to_string());
            let body = tr.protected_parts(e, rest, names, xs);
            Term::Let(name, None, Rc::new(value), Rc::new(body))
        })
    }
    pub(super) fn try_state(&mut self, body: &ir::Expr, arms: &[ir::Arm], out: &[String]) -> Term {
        self.try_state_as(body, arms, out, false)
    }
    fn try_state_as(
        &mut self,
        body: &ir::Expr,
        arms: &[ir::Arm],
        out: &[String],
        protected_result: bool,
    ) -> Term {
        let tracked = self.active_variables();
        let input_cast = self.fresh("error_action");
        let saved = self.error_action.replace(input_cast.clone());
        let protected = self.protected(body, &tracked);
        self.error_action = saved;
        let protected = self.error_actions(input_cast, protected);
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let v = self.fresh("value");
        let error = self.fresh("error");
        let control = self.control.clone();
        let good_value = if control.is_some() {
            result(true, Term::Var(v.clone()))
        } else {
            Term::Var(v.clone())
        };
        let good = if protected_result {
            self.captured(true, Term::Var(v.clone()), out)
        } else {
            self.pack(good_value, out)
        };
        let bad = self.handler_state(Term::Var(error.clone()), arms, out, protected_result);
        let pattern = |first| state_pattern(first, &names);
        let good_pat = if control.is_some() {
            rpat(true, rpat(true, Pat::Var(v)))
        } else {
            rpat(true, Pat::Var(v))
        };
        let mut matched = vec![
            Arm {
                pat: pattern(good_pat),
                guard: None,
                body: good,
            },
            Arm {
                pat: pattern(rpat(false, Pat::Var(error))),
                guard: None,
                body: bad,
            },
        ];
        if control.is_some() {
            let exit = self.fresh("exit");
            let completion = result(false, Term::Var(exit.clone()));
            let completion = if protected_result {
                result(true, completion)
            } else {
                completion
            };
            let body = self.pack(completion, out);
            matched.push(Arm {
                pat: pattern(rpat(true, rpat(false, Pat::Var(exit)))),
                guard: None,
                body,
            });
        }
        self.versions.truncate(mark);
        Term::Match(Rc::new(protected), matched.into())
    }
    fn protected_loop(&mut self, condition: &ir::Expr, body: &ir::Expr, out: &[String]) -> Term {
        let tracked = self.active_variables();
        let name = self.fresh("loop");
        let state = self.fresh("state");
        let initial = self.pack(Term::unit(), &tracked);
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let step = self.protected_with(condition, out, |tr, c| {
            let done = tr.captured(true, Term::unit(), out);
            let next = tr.protected_with(body, out, |tr, _| {
                Term::app(Term::Var(name.clone()), tr.pack(Term::unit(), &tracked))
            });
            cond(c, next, done)
        });
        self.versions.truncate(mark);
        let arm = Arm {
            pat: Pat::Tuple(
                iter::once(Pat::Wild)
                    .chain(names.into_iter().map(Pat::Var))
                    .collect(),
            ),
            guard: None,
            body: step,
        };
        let function = Term::lam(
            &state,
            Term::Match(Rc::new(Term::Var(state.clone())), Rc::from([arm])),
        );
        Term::LetRec(
            name.clone(),
            None,
            Rc::new(function),
            Rc::new(Term::app(Term::Var(name), initial)),
        )
    }
}

impl Tr {
    fn protected_arms(&mut self, value: Term, arms: &[ir::Arm], xs: &[String]) -> Term {
        let Some((arm, rest)) = arms.split_first() else {
            return Term::Die("unmatched source match".into());
        };
        let tracked = self.active_variables();
        let (next, fallback) =
            self.state_thunk(&tracked, |tr| tr.protected_arms(value.clone(), rest, xs));
        let call_next =
            |tr: &Self| Term::app(Term::Var(next.clone()), tr.pack(Term::unit(), &tracked));
        let body = if let Some(g) = &arm.guard {
            self.protected_with(g, xs, |tr, condition| {
                let yes = tr.protected(&arm.body, xs);
                let no = call_next(tr);
                cond(condition, yes, no)
            })
        } else {
            self.protected(&arm.body, xs)
        };
        let matched = Term::Match(
            Rc::new(value),
            Rc::from([
                Arm {
                    pat: pat(&arm.pat),
                    guard: None,
                    body,
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: call_next(self),
                },
            ]),
        );
        Term::Let(next, None, Rc::new(fallback), Rc::new(matched))
    }
}
