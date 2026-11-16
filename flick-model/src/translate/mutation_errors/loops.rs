//! Loop completion and typed failure with the current store.
use super::{
    Arm, Name, Pat, Rc, Term, Tr, cond, ir, list_step, none, option, pat, result, rpat, some,
    state_function, state_pattern,
};

impl Tr {
    pub(super) fn protected_source_loop(
        &mut self,
        id: &str,
        condition: Option<&ir::Expr>,
        body: &ir::Expr,
        out: &[String],
    ) -> Term {
        let nested = !self.control_loops.is_empty();
        self.control_loops.push(id.to_string());
        let tracked = self.active_variables();
        let outer = self.control.replace(id.into());
        let task = self.iteration_effectful(body)
            || condition.is_some_and(|c| self.iteration_effectful(c));
        let call = |t| if task { Term::Await(Rc::new(t)) } else { t };
        let name = self.fresh("loop");
        let arg = self.fresh("state");
        let initial = self.pack(Term::unit(), &tracked);
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let step = match condition {
            Some(c) => self.protected_with(c, &tracked, |tr, c| {
                let body = tr.protected(body, &tracked);
                let payload = tr.exit_payload(id, Some(Term::unit()));
                let stop = tr.pack(result(true, result(false, payload)), &tracked);
                cond(c, body, stop)
            }),
            None => self.protected(body, &tracked),
        };
        self.control = outer;
        self.control_loops.pop();
        let values: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        self.versions
            .extend(tracked.iter().cloned().zip(values.iter().cloned()));
        let recur = call(Term::app(
            Term::Var(name.clone()),
            self.pack(Term::unit(), &tracked),
        ));
        let value = self.fresh("break");
        let error = self.fresh("error");
        let done = self.captured(true, Term::Var(value.clone()), out);
        let failed = self.captured(false, Term::Var(error.clone()), out);
        let pattern = |p| state_pattern(p, &values);
        let mut arms = vec![Arm {
            pat: pattern(rpat(true, rpat(true, Pat::Wild))),
            guard: None,
            body: recur.clone(),
        }];
        if nested {
            let other = self.fresh("other_exit");
            let bubble = self.pack(result(true, result(false, Term::Var(other.clone()))), out);
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(true, none())))),
                guard: None,
                body: recur,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(true, some(Pat::Var(value)))))),
                guard: None,
                body: done,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(false, Pat::Var(other))))),
                guard: None,
                body: bubble,
            });
        } else {
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, none()))),
                guard: None,
                body: recur,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, some(Pat::Var(value))))),
                guard: None,
                body: done,
            });
        }
        arms.push(Arm {
            pat: pattern(rpat(false, Pat::Var(error))),
            guard: None,
            body: failed,
        });
        self.versions.truncate(mark);
        let dispatch = Term::Match(Rc::new(step), arms.into());
        let dispatch = if task {
            Term::Task(Rc::new(dispatch))
        } else {
            dispatch
        };
        let function = state_function(&arg, Pat::Wild, &names, dispatch);
        Term::LetRec(
            name.clone(),
            None,
            Rc::new(function),
            Rc::new(call(Term::app(Term::Var(name), initial))),
        )
    }
}

impl Tr {
    pub(super) fn protected_source_for(
        &mut self,
        id: &str,
        body: &ir::Expr,
        out: &[String],
    ) -> Term {
        let nested = !self.control_loops.is_empty();
        self.control_loops.push(id.to_string());
        let tracked = self.active_variables();
        let outer = self.control.replace(id.into());
        let value = self.protected(body, &tracked);
        self.control = outer;
        self.control_loops.pop();
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let error = self.fresh("error");
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let good = self.captured(true, Term::unit(), out);
        let bad = self.captured(false, Term::Var(error.clone()), out);
        let pattern = |p| state_pattern(p, &names);
        let mut arms = Vec::new();
        if nested {
            let other = self.fresh("other_exit");
            let bubble = self.pack(result(true, result(false, Term::Var(other.clone()))), out);
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(true, Pat::Wild))),
                guard: None,
                body: good.clone(),
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(true, Pat::Wild)))),
                guard: None,
                body: good,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(false, Pat::Var(other))))),
                guard: None,
                body: bubble,
            });
        } else {
            arms.push(Arm {
                pat: pattern(rpat(true, Pat::Wild)),
                guard: None,
                body: good,
            });
        }
        arms.push(Arm {
            pat: pattern(rpat(false, Pat::Var(error))),
            guard: None,
            body: bad,
        });
        self.versions.truncate(mark);
        Term::Match(Rc::new(value), arms.into())
    }
    fn protected_each_dispatch(
        &mut self,
        body: Term,
        next: Term,
        values: &[Name],
        xs: &[String],
    ) -> Term {
        let nested = self.control_loops.len() > 1;
        let exit = self.fresh("exit");
        let error = self.fresh("error");
        let payload = self.control.as_deref().map_or_else(
            || option(Some(Term::Var(exit.clone()))),
            |id| self.exit_payload(id, Some(Term::Var(exit.clone()))),
        );
        let done = self.pack(result(true, result(false, payload)), xs);
        let bad = self.captured(false, Term::Var(error.clone()), xs);
        let pattern = |p| state_pattern(p, values);
        let mut arms = vec![Arm {
            pat: pattern(rpat(true, rpat(true, Pat::Wild))),
            guard: None,
            body: next.clone(),
        }];
        if nested {
            let other = self.fresh("other_exit");
            let bubble = self.pack(
                result(true, result(false, result(false, Term::Var(other.clone())))),
                xs,
            );
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(true, none())))),
                guard: None,
                body: next,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(true, some(Pat::Var(exit)))))),
                guard: None,
                body: done,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, rpat(false, Pat::Var(other))))),
                guard: None,
                body: bubble,
            });
        } else {
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, none()))),
                guard: None,
                body: next,
            });
            arms.push(Arm {
                pat: pattern(rpat(true, rpat(false, some(Pat::Var(exit))))),
                guard: None,
                body: done,
            });
        }
        arms.push(Arm {
            pat: pattern(rpat(false, Pat::Var(error))),
            guard: None,
            body: bad,
        });
        Term::Match(Rc::new(body), arms.into())
    }

    pub(super) fn protected_each(
        &mut self,
        p: &ir::Pat,
        source: Term,
        body: &ir::Expr,
        xs: &[String],
    ) -> Term {
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
        let empty = self.captured(true, Term::unit(), xs);
        let skip = call(Term::app(
            Term::Var(name.clone()),
            self.pack(Term::Var(tail.clone()), xs),
        ));
        let body = self.protected(body, xs);
        let values: Vec<_> = xs.iter().map(|x| self.fresh(x)).collect();
        self.versions
            .extend(xs.iter().cloned().zip(values.iter().cloned()));
        let next = call(Term::app(
            Term::Var(name.clone()),
            self.pack(Term::Var(tail.clone()), xs),
        ));
        let dispatch = self.protected_each_dispatch(body, next, &values, xs);
        self.versions.truncate(mark);
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
}
