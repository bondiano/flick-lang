//! The handlers of a budget's rows: `try` takes errors the budget lacks,
//! `provide` a service it lacks.

use std::rc::Rc;

use super::term::{Cx, extend};
use super::{
    Gen, MODULE, SERVICES, boolean, error_con, implementation, int, mono, payload, service, string,
    unit,
};
use crate::syntax::{Arm, Const, Name, Pat, Path, Term};
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
        let body = self.term(cx, t, &Stage::Runtime(services.clone(), raised), s, true);
        let mut arms = Vec::new();
        if self.one_in(4) {
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
        // what the operation does runs on its own stage, `->{}`
        let empty = Stage::Runtime(Row::none(), Row::none());
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
