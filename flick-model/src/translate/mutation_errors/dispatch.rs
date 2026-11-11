//! Deferred error dispatch and guard state threading.
use super::{Arm, ModelStage, Name, Pat, Rc, Scheme, Term, Tr, Type, cond, ir, pat};

impl Tr {
    /// Select an error pattern under `try`, then run its guard/body outside
    /// that catch. A false guard calls the next arm with its updated store.
    pub(super) fn handler_state(
        &mut self,
        action: Term,
        arms: &[ir::Arm],
        out: &[String],
        protected_result: bool,
    ) -> Term {
        let Some((arm, rest)) = arms.split_first() else {
            return if protected_result {
                self.captured(false, self.cast_error_action(action), out)
            } else {
                consume_error(action)
            };
        };
        let tracked = self.active_variables();
        let next_action = if arm.guard.is_none() {
            Term::Task(Rc::new(Term::Try(
                Rc::new(Term::Await(Rc::new(action.clone()))),
                Rc::from([Arm {
                    pat: pat(&arm.pat),
                    guard: None,
                    body: Term::Die("excluded error pattern".into()),
                }]),
            )))
        } else {
            action.clone()
        };
        let (next, fallback) = self.state_thunk(&tracked, |tr| {
            tr.handler_state(next_action, rest, out, protected_result)
        });
        let call_next =
            |tr: &Self| Term::app(Term::Var(next.clone()), tr.pack(Term::unit(), &tracked));
        let control = self.control.clone();
        let run_body = |tr: &mut Self| {
            if protected_result {
                tr.protected(&arm.body, out)
            } else if let Some(id) = &control {
                tr.loop_value(&arm.body, id, out)
            } else {
                tr.joined(&arm.body, out)
            }
        };
        let choose = |tr: &mut Self, condition| {
            let yes = run_body(tr);
            cond(condition, yes, call_next(tr))
        };
        let body = match &arm.guard {
            Some(g) if protected_result => self.protected_with(g, out, choose),
            Some(g) if control.is_some() => {
                self.loop_with(g, control.as_deref().expect("control"), out, &choose)
            }
            Some(g) => self.with_value(g, choose),
            None => run_body(self),
        };
        let selected_cast = self.fresh("handler_action");
        let select_task =
            |body| Term::app(Term::Var(selected_cast.clone()), Term::Task(Rc::new(body)));
        let selected = Term::Try(
            Rc::new(consume_error(action)),
            Rc::from([
                Arm {
                    pat: pat(&arm.pat),
                    guard: None,
                    body: select_task(body),
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: select_task(call_next(self)),
                },
            ]),
        );
        let selected = self.task_actions(selected_cast, Term::Await(Rc::new(selected)), false);
        Term::Let(next, None, Rc::new(fallback), Rc::new(selected))
    }
}

/// Error actions have result Never; elimination gives any continuation type.
fn consume_error(action: Term) -> Term {
    Term::Match(Rc::new(Term::Await(Rc::new(action))), Rc::from([]))
}
impl Tr {
    pub(super) fn cast_error_action(&self, action: Term) -> Term {
        match &self.error_action {
            Some(cast) => Term::app(Term::Var(cast.clone()), action),
            None => action,
        }
    }
    pub(super) fn capture_error(&self, error: Term, xs: &[String]) -> Term {
        let action = self.cast_error_action(Term::Task(Rc::new(Term::Raise(Rc::new(error)))));
        self.captured(false, action, xs)
    }
    /// A monomorphic identity shares an open Eff error row across captures.
    /// The unused task constrains only the shape (Eff _ _ Never). No witness
    /// is awaited; each real action contains only a previously caught raise.
    pub(super) fn error_actions(&mut self, cast: Name, body: Term) -> Term {
        self.task_actions(cast, body, true)
    }
    fn task_actions(&mut self, cast: Name, body: Term, throws: bool) -> Term {
        let witness = self.fresh("error_witness");
        let action = self.fresh("action");
        let force = self.fresh("shape");
        let never_id = self.fresh("never_id");
        let value = self.fresh("never");
        let never = Type::con("Prelude", "Never");
        let signature = Scheme::new(
            Vec::new(),
            Type::fun(never.clone(), ModelStage::Pure, never),
        );
        let awaited = Term::Await(Rc::new(Term::Var(witness.clone())));
        let shape = if throws {
            Term::app(Term::Var(never_id.clone()), awaited)
        } else {
            awaited
        };
        let shape = Term::Task(Rc::new(shape));
        let force_cast = cond(
            Term::bool(true),
            Term::Var(witness.clone()),
            Term::app(Term::Var(cast.clone()), Term::Var(witness.clone())),
        );
        let body = Term::Let(force.clone(), None, Rc::new(force_cast), Rc::new(body));
        let bind_cast = Term::app(
            Term::Lam(cast, Rc::new(body)),
            Term::lam(&action, Term::Var(action.clone())),
        );
        let body = Term::Let(force, None, Rc::new(shape), Rc::new(bind_cast));
        let body = Term::Let(
            never_id,
            Some(Rc::new(signature)),
            Rc::new(Term::lam(&value, Term::Var(value.clone()))),
            Rc::new(body),
        );
        Term::app(
            Term::lam(&witness, body),
            Term::Task(Rc::new(Term::Die("unexecuted error witness".into()))),
        )
    }
}
