//! State across operation handlers. Tail-only handlers keep SSA tuples;
//! handlers that can abandon/resume the body share private runtime cells.
use std::{iter, rc::Rc};

use super::{Tr, expr::stuck};
use crate::handler::Handler;
use crate::syntax::{Arm, Const, Name, Pat, Term};
use flick_core::ir::{self, ExprKind};
use flick_core::types::OpClass;

impl Tr {
    pub(super) fn protected_handler(&mut self, h: &ir::Handler, xs: &[String]) -> Term {
        if tail_only(h) && h.errors.is_empty() {
            let effects = self.handled_effects(h);
            let body = self.protected(&h.body, xs);
            let ret = h.ret.as_ref().map(|r| self.protected_return(r, xs));
            Term::Handler(Rc::new(Handler {
                effects,
                body,
                ret,
                errors: Rc::from([]),
            }))
        } else {
            self.cell_handler(h, xs, true)
        }
    }
    fn protected_return(&mut self, e: &ir::Expr, xs: &[String]) -> Term {
        let ExprKind::Lambda { params, body } = &e.kind else {
            return stuck("a non-lambda handler return callback");
        };
        let [p] = &params[..] else {
            return stuck("a handler return callback with multiple parameters");
        };
        let state = self.fresh("state");
        let body = self.unpack_protected(Term::Var(state.clone()), xs, xs, |tr, v| {
            let body = tr.protected(body, xs);
            let callback = tr.param(&p.pat, body);
            Term::app(callback, v)
        });
        Term::lam(&state, body)
    }
    pub(super) fn loop_handler(&mut self, h: &ir::Handler, id: &str, xs: &[String]) -> Term {
        if tail_only(h) && h.errors.is_empty() {
            let effects = self.handled_effects(h);
            let body = self.loop_value(&h.body, id, xs);
            let ret = h.ret.as_ref().map(|r| self.loop_return(r, id, xs));
            Term::Handler(Rc::new(Handler {
                effects,
                body,
                ret,
                errors: Rc::from([]),
            }))
        } else {
            let e = ir::Expr::new(ExprKind::Handle(Box::new(h.clone())), h.body.span);
            self.with_value(&e, |tr, v| tr.loop_pack(true, v, xs))
        }
    }
    fn loop_return(&mut self, e: &ir::Expr, id: &str, xs: &[String]) -> Term {
        let ExprKind::Lambda { params, body } = &e.kind else {
            return stuck("a non-lambda handler return callback");
        };
        let [p] = &params[..] else {
            return stuck("a handler return callback with multiple parameters");
        };
        let state = self.fresh("state");
        let body = self.unpack_loop(Term::Var(state.clone()), xs, xs, &|tr, v| {
            let body = tr.loop_value(body, id, xs);
            let callback = tr.param(&p.pat, body);
            Term::app(callback, v)
        });
        Term::lam(&state, body)
    }
    pub(super) fn cell(&self, x: &str) -> Option<&Name> {
        self.cells
            .iter()
            .rev()
            .find(|(v, _)| v == x)
            .map(|(_, n)| n)
    }
    pub(super) fn read_cell(name: &Name) -> Term {
        Term::app(Term::konst(Const::RefGet), Term::Var(name.clone()))
    }
    pub(super) fn write_cell(&self, x: &str, value: Term, rest: Term) -> Term {
        let cell = self.cell(x).expect("model cell");
        let set = Term::apps(Term::konst(Const::RefSet), [Term::Var(cell.clone()), value]);
        Term::Let("_".into(), None, Rc::new(set), Rc::new(rest))
    }
    /// The compiler-generated return callback runs in place and may use cells.
    pub(super) fn handler_return(&mut self, e: &ir::Expr) -> Term {
        if let ExprKind::Lambda { params, body } = &e.kind {
            let mut body = self.expr(body);
            for p in params.iter().rev() {
                body = self.param(&p.pat, body);
            }
            body
        } else {
            self.expr(e)
        }
    }
    pub(super) fn joined_handler(&mut self, h: &ir::Handler, xs: &[String]) -> Term {
        if tail_only(h) && h.errors.is_empty() {
            let effects = self.handled_effects(h);
            let body = self.joined(&h.body, xs);
            let ret = h.ret.as_ref().map(|r| self.state_return(r, xs));
            Term::Handler(Rc::new(Handler {
                effects,
                body,
                ret,
                errors: Rc::from([]),
            }))
        } else {
            self.cell_handler(h, xs, false)
        }
    }
    fn state_return(&mut self, e: &ir::Expr, xs: &[String]) -> Term {
        let ExprKind::Lambda { params, body } = &e.kind else {
            return stuck("a non-lambda handler return callback");
        };
        let [p] = &params[..] else {
            return stuck("a handler return callback with multiple parameters");
        };
        let state = self.fresh("state");
        let value = self.fresh("value");
        let names: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(xs.iter().cloned().zip(names.iter().cloned()));
        let body = self.joined(body, xs);
        let body = self.param(&p.pat, body);
        let body = Term::app(body, Term::Var(value.clone()));
        self.versions.truncate(mark);
        let pattern = Pat::Tuple(
            iter::once(Pat::Var(value))
                .chain(names.into_iter().map(Pat::Var))
                .collect(),
        );
        Term::lam(
            &state,
            Term::Match(
                Rc::new(Term::Var(state.clone())),
                Rc::from([Arm {
                    pat: pattern,
                    guard: None,
                    body,
                }]),
            ),
        )
    }
    pub(super) fn cell_handler(&mut self, h: &ir::Handler, xs: &[String], protected: bool) -> Term {
        let tracked: Vec<_> = self
            .active_variables()
            .into_iter()
            .filter(|x| self.cell(x).is_none())
            .collect();
        let initial: Vec<_> = tracked
            .iter()
            .map(|x| (x.clone(), self.fresh("cell"), self.version(x)))
            .collect();
        let mark = self.cells.len();
        self.cells
            .extend(initial.iter().map(|(x, n, _)| (x.clone(), n.clone())));
        let value = Term::Handler(Rc::new(self.handler(h)));
        let value = if protected {
            self.catch_atomic(value, xs, true)
        } else {
            let v = self.fresh("value");
            Term::Let(
                v.clone(),
                None,
                Rc::new(value),
                Rc::new(self.pack(Term::Var(v), xs)),
            )
        };
        self.cells.truncate(mark);
        initial
            .into_iter()
            .rev()
            .fold(value, |body, (_, name, initial)| {
                let cell = Term::app(Term::konst(Const::RefMake), initial);
                Term::Let(name, None, Rc::new(cell), Rc::new(body))
            })
    }
}

fn tail_only(h: &ir::Handler) -> bool {
    h.effects
        .iter()
        .flat_map(|e| &e.operations)
        .all(|o| o.class == OpClass::Tail)
}
