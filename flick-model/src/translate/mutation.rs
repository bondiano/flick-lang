//! `let mut` variables by passing their values along
//! (`docs/formal/elaboration.md` §3.5): a variable is a
//! name per assignment, the one in scope where it is read. A statement whose
//! value assigns variables from outside it gives a tuple of its value and of
//! what those variables hold after it, which the rest of the block takes
//! apart: the arms of an `if`, a `match` or a `try`, and the body of a
//! `scope` or a `provide`, give their tuples where they end. The checker
//! keeps assignments out of what may run later or again. Runtime handlers
//! that can abandon or resume their body use private model cells instead.

use std::iter;
use std::rc::Rc;

use flick_core::ir::{self, BinOp, ExprKind, Stmt};

use super::Tr;
use super::expr::{cond, is_global, layer_run, pat, stuck};
use crate::syntax::{Arm, Name, Pat, Term};

impl Tr {
    /// A loop carries the current values as the argument of a recursive
    /// function. Normal completion returns the same state tuple as a branch.
    pub(super) fn loop_term(
        &mut self,
        condition: &ir::Expr,
        body: &ir::Expr,
        xs: &[String],
    ) -> Term {
        let name = self.fresh("loop");
        let state = self.fresh("state");
        let initial = self.pack(Term::unit(), xs);
        let names: Vec<Name> = xs.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(xs.iter().cloned().zip(names.iter().cloned()));
        let step = self.with_value(condition, |tr, c| {
            let done = tr.pack(Term::unit(), xs);
            let next = tr.with_value(body, |tr, _| {
                Term::app(Term::Var(name.clone()), tr.pack(Term::unit(), xs))
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
        let call = Term::app(Term::Var(name.clone()), initial);
        Term::LetRec(name, None, Rc::new(function), Rc::new(call))
    }

    /// The name the variable `x` holds its value under here.
    pub(super) fn version(&self, x: &str) -> Term {
        if let Some(cell) = self.cell(x) {
            return Self::read_cell(cell);
        }
        (self.versions.iter().rev())
            .find(|(v, _)| v == x)
            .map_or_else(
                || stuck("a read of a variable out of scope"),
                |(_, n)| Term::Var(n.clone()),
            )
    }

    /// The statements of a block and its value; with `pack`, the value as a
    /// tuple with what the variables `pack` names hold at the end.
    pub(super) fn block(
        &mut self,
        stmts: &[Stmt],
        tail: &ir::Expr,
        pack: Option<&[String]>,
    ) -> Term {
        let rc = |t: Term| Rc::new(t);
        let Some((first, rest)) = stmts.split_first() else {
            return self.with_value(tail, |tr, v| match pack {
                Some(xs) => tr.pack(v, xs),
                None => v,
            });
        };
        match first {
            Stmt::Assign(x, e) if self.cell(x).is_some() => self.with_value(e, |tr, v| {
                let rest = tr.block(rest, tail, pack);
                tr.write_cell(x, v, rest)
            }),
            Stmt::Var(x, e) | Stmt::Assign(x, e) => self.with_value(e, |tr, v| {
                let name = tr.fresh(x);
                tr.versions.push((x.clone(), name.clone()));
                let body = tr.block(rest, tail, pack);
                tr.versions.pop();
                Term::Let(name, None, rc(v), rc(body))
            }),
            Stmt::Let(ir::Pat::Bind(x), e) => self.with_value(e, |tr, v| {
                let body = tr.block(rest, tail, pack);
                Term::Let(x.as_str().into(), None, rc(v), rc(body))
            }),
            Stmt::Let(ir::Pat::Wild, e) | Stmt::Expr(e) => self.with_value(e, |tr, v| {
                let body = tr.block(rest, tail, pack);
                Term::Let("_".into(), None, rc(v), rc(body))
            }),
            Stmt::Let(p, e) => self.with_value(e, |tr, v| {
                let body = tr.block(rest, tail, pack);
                let arm = Arm {
                    pat: pat(p),
                    guard: None,
                    body,
                };
                Term::Match(rc(v), Rc::from([arm]))
            }),
            Stmt::Fun { .. } => {
                let body = self.block(rest, tail, pack);
                self.stmt(first, body)
            }
        }
    }

    /// `k` of the value of `e`. When `e` assigns variables from outside it,
    /// its tuple is taken apart first and the variables hold their new names
    /// in `k`.
    pub(super) fn with_value(
        &mut self,
        e: &ir::Expr,
        k: impl FnOnce(&mut Self, Term) -> Term,
    ) -> Term {
        let xs = modified(e);
        if xs.is_empty() {
            let v = self.expr(e);
            return k(self, v);
        }
        let joined = self.joined(e, &xs);
        let value = self.fresh("v");
        let names: Vec<Name> = xs.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(xs.iter().cloned().zip(names.iter().cloned()));
        let body = k(self, Term::Var(value.clone()));
        self.versions.truncate(mark);
        let pats = iter::once(value).chain(names).map(Pat::Var).collect();
        let arm = Arm {
            pat: Pat::Tuple(pats),
            guard: None,
            body,
        };
        Term::Match(Rc::new(joined), Rc::from([arm]))
    }

    /// `(v, x₁, …, xₙ)`: `v` and what the variables `xs` hold here.
    pub(super) fn pack(&self, v: Term, xs: &[String]) -> Term {
        let held = xs.iter().map(|x| self.version(x));
        Term::Tuple(iter::once(v).chain(held).collect())
    }

    /// `e` as the tuple of its value and of what the variables `xs` hold
    /// after it.
    pub(super) fn joined(&mut self, e: &ir::Expr, xs: &[String]) -> Term {
        match &e.kind {
            ExprKind::ControlLoop {
                id,
                body,
                iterable: true,
                ..
            } => self.source_for(id, body, xs),
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable: false,
            } => self.source_loop(id, condition.as_deref(), body, xs),
            ExprKind::Loop { condition, body } => self.loop_term(condition, body, xs),
            ExprKind::Block(stmts, tail) => self.block(stmts, tail, Some(xs)),
            ExprKind::If(c, a, b) => self.with_value(c, |tr, c| {
                let (a, b) = (tr.joined(a, xs), tr.joined(b, xs));
                cond(c, a, b)
            }),
            ExprKind::Binary(BinOp::And, a, b) => self.with_value(a, |tr, a| {
                let (b, no) = (tr.joined(b, xs), tr.pack(Term::bool(false), xs));
                cond(a, b, no)
            }),
            ExprKind::Binary(BinOp::Or, a, b) => self.with_value(a, |tr, a| {
                let (yes, b) = (tr.pack(Term::bool(true), xs), tr.joined(b, xs));
                cond(a, yes, b)
            }),
            ExprKind::Match(s, arms) => {
                self.with_value(s, |tr, value| tr.match_state(value, arms, xs))
            }
            ExprKind::Handle(h) if h.is_try() => self.try_state(&h.body, &h.errors, xs),
            ExprKind::Handle(h) => self.joined_handler(h, xs),
            ExprKind::App { func, arg, .. } if layer_run(func).is_some() => self
                .with_value(layer_run(func).expect("layer"), |tr, layer| {
                    tr.provide_value(layer, arg, &|tr, body| tr.joined(body, xs))
                }),
            ExprKind::App { func, arg, .. } if is_global(func, "Scope", "run") => {
                self.scope_with(arg, &|tr, body| tr.joined(body, xs))
            }
            ExprKind::Within(_, inner) | ExprKind::Lift(inner, _) => self.joined(inner, xs),
            ExprKind::Lambda { .. }
            | ExprKind::Task(_)
            | ExprKind::Abstract(..)
            | ExprKind::Generate { .. }
            | ExprKind::Each { .. } => stuck("an assignment in a handler, a function or a loop"),
            _ => {
                let mut children = Vec::new();
                e.each_child(&mut |c| children.push(c.clone()));
                self.hoisted(e, &children, Vec::new(), xs)
            }
        }
    }

    /// `e`, whose parts are all evaluated, in order, each once: each part
    /// is bound to a name first, so that what one assigns the next sees.
    fn hoisted(
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
                let name = names.next().expect("a name per part");
                *c = ir::Expr::new(ExprKind::Local(name), c.span);
            });
            let v = self.expr(&rebuilt);
            return self.pack(v, xs);
        };
        self.with_value(first, |tr, v| {
            let name = tr.fresh("t");
            names.push(name.to_string());
            let body = tr.hoisted(e, rest, names, xs);
            Term::Let(name, None, Rc::new(v), Rc::new(body))
        })
    }
}

/// The variables `e` assigns that it does not declare, by name.
pub fn modified(e: &ir::Expr) -> Vec<String> {
    fn walk(e: &ir::Expr, assigned: &mut Vec<String>, declared: &mut Vec<String>) {
        if let ExprKind::Block(stmts, _) = &e.kind {
            for s in stmts {
                match s {
                    Stmt::Var(x, _) => declared.push(x.clone()),
                    Stmt::Assign(x, _) => assigned.push(x.clone()),
                    _ => {}
                }
            }
        }
        e.each_child(&mut |c| walk(c, assigned, declared));
    }
    let (mut assigned, mut declared) = (Vec::new(), Vec::new());
    walk(e, &mut assigned, &mut declared);
    assigned.retain(|x| !declared.contains(x));
    assigned.sort();
    assigned.dedup();
    assigned
}

impl Tr {
    fn match_state(&mut self, value: Term, arms: &[ir::Arm], out: &[String]) -> Term {
        let Some((arm, rest)) = arms.split_first() else {
            return Term::Die("unmatched source match".into());
        };
        let tracked = self.active_variables();
        let (next, fallback) =
            self.state_thunk(&tracked, |tr| tr.match_state(value.clone(), rest, out));
        let call_next =
            |tr: &Self| Term::app(Term::Var(next.clone()), tr.pack(Term::unit(), &tracked));
        let branch = if let Some(g) = &arm.guard {
            self.with_value(g, |tr, c| {
                let yes = tr.joined(&arm.body, out);
                cond(c, yes, call_next(tr))
            })
        } else {
            self.joined(&arm.body, out)
        };
        let matched = Term::Match(
            Rc::new(value),
            Rc::from([
                Arm {
                    pat: pat(&arm.pat),
                    guard: None,
                    body: branch,
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
