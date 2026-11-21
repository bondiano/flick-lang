//! A loop body returns either its value or a lexical exit, alongside state.
//! Exit values cross scope callbacks as ordinary results, so scopes close normally.
mod for_each;

use super::{
    Tr,
    expr::{cond, is_lang, layer_run, pat, stuck},
    mutation::modified,
};
use crate::syntax::{Arm, Con, Const, Name, Pat, Path, Term};
use flick_core::ir::{self, ExprKind, Stmt};
use flick_core::types::lang::Lang;
use std::iter;
use std::rc::Rc;
fn con(ty: &str, name: &str, arity: usize) -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new("Prelude", ty),
        name: name.into(),
        arity,
        dicts: 0,
    })
}
pub(super) fn option(value: Option<Term>) -> Term {
    value.map_or_else(
        || Term::konst_con(con("Option", "None", 0)),
        |v| Term::app(Term::konst_con(con("Option", "Some", 1)), v),
    )
}
pub(super) fn result(ok: bool, value: Term) -> Term {
    Term::app(
        Term::konst_con(con("Result", if ok { "Ok" } else { "Err" }, 1)),
        value,
    )
}
pub(super) fn rpat(ok: bool, p: Pat) -> Pat {
    Pat::Con(
        con("Result", if ok { "Ok" } else { "Err" }, 1),
        Rc::from([p]),
    )
}
pub(super) fn some(p: Pat) -> Pat {
    Pat::Con(con("Option", "Some", 1), Rc::from([p]))
}
pub(super) fn none() -> Pat {
    Pat::Con(con("Option", "None", 0), Rc::from([]))
}
pub(super) fn effectful(e: &ir::Expr) -> bool {
    if matches!(
        e.kind,
        ExprKind::App {
            stage: ir::Stage::Runtime | ir::Stage::Sync,
            ..
        } | ExprKind::Await(_)
            | ExprKind::Handle(_)
            | ExprKind::Generate {
                strict: false,
                sync: true,
                ..
            }
    ) {
        return true;
    }
    if let ExprKind::Generate {
        strict: false,
        sync: false,
        limit,
        ..
    } = &e.kind
    {
        return limit.as_deref().is_some_and(effectful);
    }
    if matches!(e.kind, ExprKind::Lambda { .. } | ExprKind::Task(_)) {
        return false;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= effectful(c));
    found
}

pub(super) fn state_pattern(first: Pat, names: &[Name]) -> Pat {
    Pat::Tuple(
        iter::once(first)
            .chain(names.iter().cloned().map(Pat::Var))
            .collect(),
    )
}
pub(super) fn state_function(arg: &Name, first: Pat, names: &[Name], body: Term) -> Term {
    Term::lam(
        arg,
        Term::Match(
            Rc::new(Term::Var(arg.clone())),
            Rc::from([Arm {
                pat: state_pattern(first, names),
                guard: None,
                body,
            }]),
        ),
    )
}

pub(super) fn seq_iter(source: Term) -> Term {
    Term::app(Term::konst(Const::SeqIter), source)
}

pub(super) fn seq_step(seq: &Name, head: Name, tail: Name, empty: Term, matched: Term) -> Term {
    Term::Match(
        Rc::new(Term::app(
            Term::konst(Const::SeqNext),
            Term::Var(seq.clone()),
        )),
        Rc::from([
            Arm {
                pat: none(),
                guard: None,
                body: empty,
            },
            Arm {
                pat: some(Pat::Tuple(Rc::from([Pat::Var(head), Pat::Var(tail)]))),
                guard: None,
                body: matched,
            },
        ]),
    )
}

impl Tr<'_> {
    pub(super) fn exit_payload(&self, target: &str, value: Option<Term>) -> Term {
        let opt = option(value);
        let Some(pos) = self.control_loops.iter().rposition(|id| id == target) else {
            return opt;
        };
        let mut payload = if pos > 0 { result(true, opt) } else { opt };
        for _ in (pos + 1)..self.control_loops.len() {
            payload = result(false, payload);
        }
        payload
    }
    pub(super) fn loop_pack(&self, ok: bool, v: Term, xs: &[String]) -> Term {
        self.pack(result(ok, v), xs)
    }
    pub(super) fn loop_with(
        &mut self,
        e: &ir::Expr,
        id: &str,
        out: &[String],
        k: &dyn Fn(&mut Self, Term) -> Term,
    ) -> Term {
        if !matches!(
            e.kind,
            ExprKind::ControlLoop { .. } | ExprKind::LoopExit { .. }
        ) && modified(e).is_empty()
            && !self.loop_completion(e, id)
        {
            let value = self.expr(e);
            return k(self, value);
        }
        let tracked = self.active_variables();
        let value = self.loop_value(e, id, &tracked);
        self.unpack_loop(value, &tracked, out, k)
    }
    pub(super) fn unpack_loop(
        &mut self,
        value: Term,
        tracked: &[String],
        out: &[String],
        k: &dyn Fn(&mut Self, Term) -> Term,
    ) -> Term {
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let v = self.fresh("value");
        let exit = self.fresh("exit");
        let good = k(self, Term::Var(v.clone()));
        let bad = self.loop_pack(false, Term::Var(exit.clone()), out);
        self.versions.truncate(mark);
        let pattern = |p| state_pattern(p, &names);
        Term::Match(
            Rc::new(value),
            Rc::from([
                Arm {
                    pat: pattern(rpat(true, Pat::Var(v))),
                    guard: None,
                    body: good,
                },
                Arm {
                    pat: pattern(rpat(false, Pat::Var(exit))),
                    guard: None,
                    body: bad,
                },
            ]),
        )
    }
    fn loop_block(&mut self, stmts: &[Stmt], tail: &ir::Expr, id: &str, xs: &[String]) -> Term {
        let Some((first, rest)) = stmts.split_first() else {
            return self.loop_value(tail, id, xs);
        };
        match first {
            Stmt::Assign(x, e) if self.cell(x).is_some() => self.loop_with(e, id, xs, &|tr, v| {
                let rest = tr.loop_block(rest, tail, id, xs);
                tr.write_cell(x, v, rest)
            }),
            Stmt::Var(x, e) | Stmt::Assign(x, e) => self.loop_with(e, id, xs, &|tr, v| {
                let name = tr.fresh(x);
                tr.versions.push((x.clone(), name.clone()));
                let body = tr.loop_block(rest, tail, id, xs);
                tr.versions.pop();
                Term::Let(name, None, Rc::new(v), Rc::new(body))
            }),
            Stmt::Let(ir::Pat::Bind(x), e) => self.loop_with(e, id, xs, &|tr, v| {
                let body = tr.loop_block(rest, tail, id, xs);
                if let Some(sig) = tr.local_scheme(x) {
                    Term::Let(x.as_str().into(), Some(sig), Rc::new(v), Rc::new(body))
                } else {
                    Term::Match(
                        Rc::new(v),
                        Rc::from([Arm {
                            pat: Pat::Var(x.as_str().into()),
                            guard: None,
                            body,
                        }]),
                    )
                }
            }),
            Stmt::Let(p, e) => self.loop_with(e, id, xs, &|tr, v| {
                let body = tr.loop_block(rest, tail, id, xs);
                Term::Match(
                    Rc::new(v),
                    Rc::from([Arm {
                        pat: pat(p),
                        guard: None,
                        body,
                    }]),
                )
            }),
            Stmt::Expr(e) => self.loop_with(e, id, xs, &|tr, v| {
                let body = tr.loop_block(rest, tail, id, xs);
                Term::Let("_".into(), None, Rc::new(v), Rc::new(body))
            }),
            Stmt::Fun { .. } => {
                let body = self.loop_block(rest, tail, id, xs);
                self.stmt(first, body)
            }
        }
    }
    pub(super) fn loop_value(&mut self, e: &ir::Expr, id: &str, xs: &[String]) -> Term {
        if !matches!(
            e.kind,
            ExprKind::Each { .. }
                | ExprKind::Yield(_)
                | ExprKind::ControlLoop { .. }
                | ExprKind::LoopExit { .. }
        ) && modified(e).is_empty()
            && !self.loop_completion(e, id)
        {
            let value = self.expr(e);
            return self.loop_pack(true, value, xs);
        }
        if let Some(value) = self.loop_application(e, id, xs) {
            return value;
        }
        match &e.kind {
            ExprKind::App { func, arg, .. } if matches!(&func.kind, ExprKind::Local(n) if self.resumptions.iter().any(|(r, target)| r == n && target == id)) => {
                self.loop_with(arg, id, xs, &|tr, v| {
                    let resume = Term::app(tr.expr(func), v);
                    tr.pack(resume, xs)
                })
            }
            ExprKind::LoopExit { id: target, value } => {
                if let Some(value) = value {
                    self.loop_with(value, id, xs, &|tr, v| {
                        let payload = tr.exit_payload(target, Some(v));
                        tr.loop_pack(false, payload, xs)
                    })
                } else {
                    let payload = self.exit_payload(target, None);
                    self.loop_pack(false, payload, xs)
                }
            }
            ExprKind::Handle(h) if h.is_try() => {
                let saved = self.control.replace(id.into());
                let value = self.try_state(&h.body, &h.errors, xs);
                self.control = saved;
                value
            }
            ExprKind::Handle(h) => self.loop_handler(h, id, xs),
            ExprKind::Each {
                stop,
                pat: p,
                source,
                body,
            } => self.loop_each(stop.is_some(), p, source, body, id, xs),
            ExprKind::Block(stmts, tail) => self.loop_block(stmts, tail, id, xs),
            ExprKind::If(c, a, b) => self.loop_with(c, id, xs, &|tr, c| {
                let a = tr.loop_value(a, id, xs);
                let b = tr.loop_value(b, id, xs);
                cond(c, a, b)
            }),
            ExprKind::Binary(op @ (ir::BinOp::And | ir::BinOp::Or), c, r) => {
                self.loop_with(c, id, xs, &|tr, c| {
                    let rhs = tr.loop_value(r, id, xs);
                    let other = tr.loop_pack(true, Term::bool(*op == ir::BinOp::Or), xs);
                    if *op == ir::BinOp::And {
                        cond(c, rhs, other)
                    } else {
                        cond(c, other, rhs)
                    }
                })
            }
            ExprKind::Match(s, arms) => self.loop_with(s, id, xs, &|tr, value| {
                let name = tr.fresh("scrutinee");
                let body = tr.loop_arms(Term::Var(name.clone()), arms, id, xs);
                Term::Let(name, None, Rc::new(value), Rc::new(body))
            }),
            ExprKind::App { func, arg, .. } if is_lang(func, Lang::ScopeRun) => {
                self.scope_with(arg, &|tr, body| tr.loop_value(body, id, xs))
            }
            ExprKind::App { func, arg, .. } if layer_run(func).is_some() => {
                self.loop_with(layer_run(func).expect("layer"), id, xs, &|tr, layer| {
                    tr.provide_value(layer, arg, &|tr, body| tr.loop_value(body, id, xs))
                })
            }
            ExprKind::ControlLoop {
                id: child_id,
                condition,
                body,
                iterable,
            } => self.nested_control_loop(child_id, condition.as_deref(), body, *iterable, xs),
            ExprKind::Lambda { .. }
            | ExprKind::Abstract(..)
            | ExprKind::Task(_)
            | ExprKind::Generate { .. } => {
                // A nested source loop consumes only its own exits.
                self.with_value(e, |tr, v| tr.loop_pack(true, v, xs))
            }
            ExprKind::Dicts(inner, _) if matches!(inner.kind, ExprKind::Perform { .. }) => {
                self.with_value(e, |tr, v| tr.loop_pack(true, v, xs))
            }
            _ => {
                let mut parts = Vec::new();
                e.each_child(&mut |c| parts.push(c.clone()));
                self.loop_parts(e, &parts, &[], id, xs)
            }
        }
    }
    fn loop_each(
        &mut self,
        has_stop: bool,
        p: &ir::Pat,
        source: &ir::Expr,
        body: &ir::Expr,
        id: &str,
        xs: &[String],
    ) -> Term {
        if has_stop {
            self.loop_pack(true, stuck("a `for` loop with a stop predicate"), xs)
        } else {
            self.loop_with(source, id, xs, &|tr, source| {
                tr.source_each(p, source, body, id, xs)
            })
        }
    }

    fn nested_control_loop(
        &mut self,
        child_id: &str,
        condition: Option<&ir::Expr>,
        body: &ir::Expr,
        iterable: bool,
        xs: &[String],
    ) -> Term {
        let tracked = self.active_variables();
        let value = if iterable {
            self.source_for(child_id, body, &tracked)
        } else {
            self.source_loop(child_id, condition, body, &tracked)
        };
        self.unpack_loop(value, &tracked, xs, &|tr, v| tr.loop_pack(true, v, xs))
    }

    fn loop_parts(
        &mut self,
        e: &ir::Expr,
        parts: &[ir::Expr],
        names: &[String],
        id: &str,
        xs: &[String],
    ) -> Term {
        let Some((first, rest)) = parts.split_first() else {
            let mut rebuilt = e.clone();
            let mut names = names.iter();
            rebuilt.each_child_mut(&mut |c| {
                *c = ir::Expr::new(ExprKind::Local(names.next().expect("part").clone()), c.span);
            });
            let value = self.expr(&rebuilt);
            return self.loop_pack(true, value, xs);
        };
        self.loop_with(first, id, xs, &|tr, value| {
            let name = tr.fresh("operand");
            let mut names = names.to_vec();
            names.push(name.to_string());
            let body = tr.loop_parts(e, rest, &names, id, xs);
            Term::Let(name, None, Rc::new(value), Rc::new(body))
        })
    }
    pub(super) fn source_loop(
        &mut self,
        id: &str,
        condition: Option<&ir::Expr>,
        body: &ir::Expr,
        xs: &[String],
    ) -> Term {
        let nested = !self.control_loops.is_empty();
        self.control_loops.push(id.to_string());
        let effectful = self.iteration_effectful(body)
            || condition.is_some_and(|c| self.iteration_effectful(c));
        let call = |t| {
            if effectful {
                Term::Await(Rc::new(t))
            } else {
                t
            }
        };
        let name = self.fresh("loop");
        let state = self.fresh("state");
        let initial = self.pack(Term::unit(), xs);
        let names: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(xs.iter().cloned().zip(names.iter().cloned()));
        let step = if let Some(c) = condition {
            self.loop_with(c, id, xs, &|tr, c| {
                let payload = tr.exit_payload(id, Some(Term::unit()));
                let done = tr.loop_pack(false, payload, xs);
                let body = tr.loop_value(body, id, xs);
                cond(c, body, done)
            })
        } else {
            self.loop_value(body, id, xs)
        };
        let values: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        self.versions
            .extend(xs.iter().cloned().zip(values.iter().cloned()));
        let recur = call(Term::app(
            Term::Var(name.clone()),
            self.pack(Term::unit(), xs),
        ));
        let v = self.fresh("break");
        let done = if nested {
            self.loop_pack(true, Term::Var(v.clone()), xs)
        } else {
            self.pack(Term::Var(v.clone()), xs)
        };
        let pattern = |p| state_pattern(p, &values);
        let mut arms = vec![Arm {
            pat: pattern(rpat(true, Pat::Wild)),
            guard: None,
            body: recur.clone(),
        }];
        if nested {
            let other = self.fresh("other_exit");
            let bubble = self.loop_pack(false, Term::Var(other.clone()), xs);
            arms.push(Arm {
                pat: pattern(rpat(false, rpat(true, none()))),
                guard: None,
                body: recur,
            });
            arms.push(Arm {
                pat: pattern(rpat(false, rpat(true, some(Pat::Var(v))))),
                guard: None,
                body: done,
            });
            arms.push(Arm {
                pat: pattern(rpat(false, rpat(false, Pat::Var(other)))),
                guard: None,
                body: bubble,
            });
        } else {
            arms.push(Arm {
                pat: pattern(rpat(false, none())),
                guard: None,
                body: recur,
            });
            arms.push(Arm {
                pat: pattern(rpat(false, some(Pat::Var(v)))),
                guard: None,
                body: done,
            });
        }
        self.versions.truncate(mark);
        let dispatch = Term::Match(Rc::new(step), arms.into());
        let dispatch = if effectful {
            Term::Task(Rc::new(dispatch))
        } else {
            dispatch
        };
        let function = state_function(&state, Pat::Wild, &names, dispatch);
        self.control_loops.pop();
        Term::LetRec(
            name.clone(),
            None,
            Rc::new(function),
            Rc::new(call(Term::app(Term::Var(name), initial))),
        )
    }
}
