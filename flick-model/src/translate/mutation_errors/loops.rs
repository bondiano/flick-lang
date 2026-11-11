//! Loop completion and typed failure with the current store.
use super::{
    Arm, Pat, Rc, Term, Tr, cond, effectful, ir, list_step, none, option, pat, result, result_pat,
    rpat, some, state_function, state_pattern,
};

impl Tr {
    pub(super) fn protected_source_loop(
        &mut self,
        id: &str,
        condition: Option<&ir::Expr>,
        body: &ir::Expr,
        out: &[String],
    ) -> Term {
        let tracked = self.active_variables();
        let outer = self.control.replace(id.into());
        let task = effectful(body) || condition.is_some_and(effectful);
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
                let stop = tr.pack(
                    result(true, result(false, option(Some(Term::unit())))),
                    &tracked,
                );
                cond(c, body, stop)
            }),
            None => self.protected(body, &tracked),
        };
        self.control = outer;
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
        self.versions.truncate(mark);
        let pattern = |p| state_pattern(p, &values);
        let dispatch = Term::Match(
            Rc::new(step),
            Rc::from([
                Arm {
                    pat: pattern(rpat(true, rpat(true, Pat::Wild))),
                    guard: None,
                    body: recur.clone(),
                },
                Arm {
                    pat: pattern(rpat(true, rpat(false, none()))),
                    guard: None,
                    body: recur,
                },
                Arm {
                    pat: pattern(rpat(true, rpat(false, some(Pat::Var(value))))),
                    guard: None,
                    body: done,
                },
                Arm {
                    pat: pattern(result_pat(false, error)),
                    guard: None,
                    body: failed,
                },
            ]),
        );
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
        let tracked = self.active_variables();
        let outer = self.control.replace(id.into());
        let value = self.protected(body, &tracked);
        self.control = outer;
        let names: Vec<_> = tracked.iter().map(|x| self.fresh(x)).collect();
        let error = self.fresh("error");
        let mark = self.versions.len();
        self.versions
            .extend(tracked.iter().cloned().zip(names.iter().cloned()));
        let good = self.captured(true, Term::unit(), out);
        let bad = self.captured(false, Term::Var(error.clone()), out);
        self.versions.truncate(mark);
        let pattern = |p| state_pattern(p, &names);
        Term::Match(
            Rc::new(value),
            Rc::from([
                Arm {
                    pat: pattern(rpat(true, Pat::Wild)),
                    guard: None,
                    body: good,
                },
                Arm {
                    pat: pattern(result_pat(false, error)),
                    guard: None,
                    body: bad,
                },
            ]),
        )
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
        let task = effectful(body);
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
        let exit = self.fresh("exit");
        let error = self.fresh("error");
        let done = self.pack(
            result(true, result(false, option(Some(Term::Var(exit.clone()))))),
            xs,
        );
        let bad = self.captured(false, Term::Var(error.clone()), xs);
        self.versions.truncate(mark);
        let pattern = |p| state_pattern(p, &values);
        let dispatch = Term::Match(
            Rc::new(body),
            Rc::from([
                Arm {
                    pat: pattern(rpat(true, rpat(true, Pat::Wild))),
                    guard: None,
                    body: next.clone(),
                },
                Arm {
                    pat: pattern(rpat(true, rpat(false, none()))),
                    guard: None,
                    body: next,
                },
                Arm {
                    pat: pattern(rpat(true, rpat(false, some(Pat::Var(exit))))),
                    guard: None,
                    body: done,
                },
                Arm {
                    pat: pattern(result_pat(false, error)),
                    guard: None,
                    body: bad,
                },
            ]),
        );
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
