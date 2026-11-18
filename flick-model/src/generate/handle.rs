//! The handlers of a budget's rows: `try` takes errors the budget lacks,
//! `provide` a service it lacks. An arm `x` of a `try` may take the errors
//! apart with a `match` (Match-Err).

use std::collections::BTreeSet;
use std::rc::Rc;

use super::regions::visible;

use super::term::{Cx, extend};
use super::{
    Gen, MODULE, SERVICES, boolean, error_con, implementation, int, mono, payload, service, string,
    unit,
};
use crate::syntax::{Arm, Const, Lit, Name, Pat, Path, Term};
use crate::types::{Row, Stage, Type};

impl Gen<'_> {
    /// (Try): the body may raise errors the budget lacks, the arms take
    /// them all, each one or with `_`; an arm may have a guard.
    pub(super) fn try_(&mut self, cx: &Cx, t: &Type, services: &Row, errors: &Row, s: u32) -> Term {
        let budget = Stage::Runtime(services.clone(), errors.clone());
        let caught = [&["Oops"][..], &["Bad"], &["Oops", "Bad"]][self.pick(3)];
        let mut raised = errors.clone();
        for e in caught {
            raised.elems.insert(Path::new(MODULE, e), Rc::from([]));
        }
        let inner = Stage::Runtime(services.clone(), raised.clone());
        let body = self.term(cx, t, &inner, s, true);
        let mut arms = Vec::new();
        if self.one_in(4) {
            if self.one_in(2) {
                return self.match_errors(cx, t, (&budget, &inner), &raised, body, s);
            }
            arms.push(Arm {
                pat: Pat::Wild,
                guard: None,
                body: self.term(cx, t, &budget, s, true),
            });
            return Term::Try(Rc::new(body), arms.into());
        }
        for e in caught {
            let x = self.fresh("x");
            let inner = extend(cx, &x, mono(payload(e)));
            if self.one_in(3) {
                let guard = self.term(&inner, &boolean(), &budget, s, false);
                let pat = Pat::Con(error_con(e), Rc::from([Pat::Var(x.clone())]));
                let body = self.term(&inner, t, &budget, s, true);
                arms.push(Arm {
                    pat,
                    guard: Some(guard),
                    body,
                });
            }
            let pat = Pat::Con(error_con(e), Rc::from([Pat::Var(x)]));
            let body = self.term(&inner, t, &budget, s, true);
            arms.push(Arm {
                pat,
                guard: None,
                body,
            });
        }
        Term::Try(Rc::new(body), arms.into())
    }

    /// (Match-Err): an arm `x` takes every error of the body, which raises
    /// each error of `raised` after a test, so that `x : Errors raised` in
    /// both checkers; a `match` takes `x` apart by them.
    fn match_errors(
        &mut self,
        cx: &Cx,
        t: &Type,
        (budget, inner): (&Stage, &Stage),
        raised: &Row,
        mut body: Term,
        s: u32,
    ) -> Term {
        let names: Vec<Name> = raised.elems.keys().map(|e| e.name.clone()).collect();
        for e in names.iter().rev() {
            let test = self.term(cx, &boolean(), inner, s, false);
            let v = self.term(cx, &payload(e), inner, s, false);
            let error = Term::app(Term::konst_con(error_con(e)), v);
            let raise = Term::app(Term::konst(Const::Raise), error);
            let arm = |b: bool, body: Term| Arm {
                pat: Pat::Lit(Lit::Bool(b)),
                guard: None,
                body,
            };
            body = Term::Match(
                Rc::new(test),
                Rc::from([arm(true, raise), arm(false, body)]),
            );
        }
        let x = self.fresh("x");
        let mut arms = Vec::new();
        for e in &names {
            let y = self.fresh("y");
            let bound = extend(cx, &y, mono(payload(e)));
            arms.push(Arm {
                pat: Pat::Con(error_con(e), Rc::from([Pat::Var(y)])),
                guard: None,
                body: self.term(&bound, t, budget, s, true),
            });
        }
        let handler = Term::Match(Rc::new(Term::Var(x.clone())), arms.into());
        let arm = Arm {
            pat: Pat::Var(x),
            guard: None,
            body: handler,
        };
        Term::Try(Rc::new(body), Rc::from([arm]))
    }

    /// (Provide) with `Layer.succeed` or `Layer.effect`; the body may use
    /// the service the layer gives.
    pub(super) fn provide(
        &mut self,
        cx: &Cx,
        t: &Type,
        services: &Row,
        errors: &Row,
        s: u32,
    ) -> Term {
        let budget = Stage::Runtime(services.clone(), errors.clone());
        let name = SERVICES[self.pick(2)];
        let mut needed = services.clone();
        needed.elems.insert(service(name), Rc::from([]));
        let body = self.term(cx, t, &Stage::Runtime(needed, errors.clone()), s, true);
        // what the operation does runs on its own stage, `->{}`, and what
        // the layer builds holds no region
        let empty = Stage::Runtime(Row::none(), Row::none());
        let cx = &visible(cx, &BTreeSet::new());
        let op = if name == "Log" {
            let line: Name = "line".into();
            self.term(&extend(cx, &line, mono(string())), &unit(), &empty, s, true)
        } else {
            let u: Name = "u".into();
            self.term(&extend(cx, &u, mono(unit())), &int(), &empty, s, true)
        };
        let imp = implementation(name, Some(op));
        let layer = if self.pick(2) == 0 {
            Term::app(Term::konst(Const::LayerSucceed(service(name))), imp)
        } else {
            let build = self.term(cx, &unit(), &budget, s, false);
            let task = Term::Task(Rc::new(Term::Let(
                "_".into(),
                Some(Rc::new(mono(unit()))),
                Rc::new(build),
                Rc::new(imp),
            )));
            Term::app(Term::konst(Const::LayerEffect(service(name))), task)
        };
        Term::Provide(Rc::new(layer), Rc::new(body))
    }
}
