//! Iterable `for` / `each` control loops, match arm continuations, and state thunks.
use super::super::Tr;
use super::super::expr::{cond, pat};
use super::{list_step, none, result, rpat, some, state_function, state_pattern};
use crate::syntax::{Arm, Name, Pat, Term};
use flick_core::ir;
use std::iter;
use std::rc::Rc;

impl Tr<'_> {
    pub(in crate::translate) fn source_for(
        &mut self,
        id: &str,
        body: &ir::Expr,
        xs: &[String],
    ) -> Term {
        let nested = !self.control_loops.is_empty();
        self.control_loops.push(id.to_string());
        let value = self.loop_value(body, id, xs);
        let names: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(xs.iter().cloned().zip(names.iter().cloned()));
        let done = if nested {
            self.loop_pack(true, Term::unit(), xs)
        } else {
            self.pack(Term::unit(), xs)
        };
        let pattern = |first| {
            Pat::Tuple(
                iter::once(first)
                    .chain(names.iter().cloned().map(Pat::Var))
                    .collect(),
            )
        };
        let arms: Rc<[Arm]> = if nested {
            let other = self.fresh("other_exit");
            let bubble = self.loop_pack(false, Term::Var(other.clone()), xs);
            Rc::from([
                Arm {
                    pat: pattern(rpat(false, rpat(true, Pat::Wild))),
                    guard: None,
                    body: done.clone(),
                },
                Arm {
                    pat: pattern(rpat(false, rpat(false, Pat::Var(other)))),
                    guard: None,
                    body: bubble,
                },
                Arm {
                    pat: pattern(Pat::Wild),
                    guard: None,
                    body: done,
                },
            ])
        } else {
            Rc::from([Arm {
                pat: pattern(Pat::Wild),
                guard: None,
                body: done,
            }])
        };
        self.versions.truncate(mark);
        self.control_loops.pop();
        Term::Match(Rc::new(value), arms)
    }

    pub(super) fn source_each(
        &mut self,
        p: &ir::Pat,
        source: Term,
        body: &ir::Expr,
        id: &str,
        xs: &[String],
    ) -> Term {
        let nested = self.control_loops.len() > 1;
        let name = self.fresh("each");
        let arg = self.fresh("state");
        let list = self.fresh("list");
        let head = self.fresh("head");
        let tail = self.fresh("tail");
        let task = self.iteration_effectful(body);
        let call = |t| if task { Term::Await(Rc::new(t)) } else { t };
        let initial = self.pack(source, xs);
        let names: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(xs.iter().cloned().zip(names.iter().cloned()));
        let empty = self.loop_pack(true, Term::unit(), xs);
        let skip = call(Term::app(
            Term::Var(name.clone()),
            self.pack(Term::Var(tail.clone()), xs),
        ));
        let body = self.loop_value(body, id, xs);
        let values: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        self.versions
            .extend(xs.iter().cloned().zip(values.iter().cloned()));
        let next = call(Term::app(
            Term::Var(name.clone()),
            self.pack(Term::Var(tail.clone()), xs),
        ));
        let exit = self.fresh("exit");
        let payload = self.exit_payload(id, Some(Term::Var(exit.clone())));
        let done = self.loop_pack(false, payload, xs);
        let pattern = |p| state_pattern(p, &values);
        let mut arms = vec![Arm {
            pat: pattern(rpat(true, Pat::Wild)),
            guard: None,
            body: next.clone(),
        }];
        if nested {
            let other = self.fresh("other_exit");
            let bubble = self.loop_pack(false, result(false, Term::Var(other.clone())), xs);
            arms.push(Arm {
                pat: pattern(rpat(false, rpat(true, none()))),
                guard: None,
                body: next,
            });
            arms.push(Arm {
                pat: pattern(rpat(false, rpat(true, some(Pat::Var(exit))))),
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
                body: next,
            });
            arms.push(Arm {
                pat: pattern(rpat(false, some(Pat::Var(exit)))),
                guard: None,
                body: done,
            });
        }
        self.versions.truncate(mark);
        let dispatch = Term::Match(Rc::new(body), arms.into());
        let matched = Term::Match(
            Rc::new(Term::Var(head.clone())),
            Rc::from([
                Arm {
                    pat: pat(p),
                    guard: None,
                    body: dispatch,
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: skip,
                },
            ]),
        );
        let step = list_step(&list, head, tail, empty, matched);
        let step = if task {
            Term::Task(Rc::new(step))
        } else {
            step
        };
        let function = state_function(&arg, Pat::Var(list), &names, step);
        Term::LetRec(
            name.clone(),
            None,
            Rc::new(function),
            Rc::new(call(Term::app(Term::Var(name), initial))),
        )
    }

    pub(super) fn loop_arms(
        &mut self,
        value: Term,
        arms: &[ir::Arm],
        id: &str,
        xs: &[String],
    ) -> Term {
        let Some((arm, rest)) = arms.split_first() else {
            return Term::Die("unmatched source match".into());
        };
        let tracked = self.active_variables();
        let (next, fallback) =
            self.state_thunk(&tracked, |tr| tr.loop_arms(value.clone(), rest, id, xs));
        let call_next =
            |tr: &Self| Term::app(Term::Var(next.clone()), tr.pack(Term::unit(), &tracked));
        let body = if let Some(g) = &arm.guard {
            self.loop_with(g, id, xs, &|tr, condition| {
                let yes = tr.loop_value(&arm.body, id, xs);
                let no = call_next(tr);
                cond(condition, yes, no)
            })
        } else {
            self.loop_value(&arm.body, id, xs)
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

    /// Bind a continuation to the current monomorphic state shape once.
    pub(in crate::translate) fn state_thunk(
        &mut self,
        tracked: &[String],
        build: impl FnOnce(&mut Self) -> Term,
    ) -> (Name, Term) {
        let name = self.fresh("next");
        let arg = self.fresh("state");
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let body = build(self);
        self.versions.truncate(mark);
        (name, state_function(&arg, Pat::Wild, &names, body))
    }
}
