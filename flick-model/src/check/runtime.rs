//! The forms of the run time (`docs/formal/core.md` §3.6): what a
//! configuration `⟨μ, E[e]⟩` holds besides source forms, typed so that
//! Preservation can be checked on every state the machine passes through
//! (X2). `Ψ` is a meta per cell, made before anything else is checked, so
//! `Gen` never quantifies a cell's type (T3).

use std::mem;
use std::rc::Rc;

use super::expr::mono;
use super::{Check, Checker, error};
use crate::decls::Decls;
use crate::syntax::{Const, Env, Term};
use crate::types::{Row, Stage, Type, Var};

/// `·;Ψ ⊢ e : τ ! φ` for some `Ψ` with `⊢ μ : Ψ`: a configuration of the
/// machine against the type of the program. Returns the least `φ`: a row
/// still unknown once everything is settled is `none` (§2.1).
pub fn check_config(decls: &Decls, mem: &[Term], e: &Term, expected: &Type) -> Check<Stage> {
    let mut c = Checker::new(decls);
    c.config = true;
    c.store = (0..mem.len()).map(|_| c.fresh()).collect();
    for (l, v) in mem.iter().enumerate() {
        let t = c.store[l].clone();
        let outer = mem::replace(&mut c.stage, Stage::Pure);
        c.check(v, &t)?;
        if mem::replace(&mut c.stage, outer) != Stage::Pure {
            return error("μ", format!("the value in ℓ{l} is not pure"));
        }
    }
    c.check(e, expected)?;
    c.finish()?;
    Ok(match c.zonk_stage(&c.stage.clone()) {
        Stage::Runtime(r, e) => Stage::Runtime(least(r), least(e)),
        Stage::Pure => Stage::Pure,
    })
}

fn least(mut r: Row) -> Row {
    r.vars.retain(|v| !matches!(v, Var::Meta(_)));
    r
}

impl Checker<'_> {
    /// The rules of §3.6, and `raise v` of a configuration.
    pub(super) fn runtime_form(&mut self, e: &Term, expected: Option<&Type>) -> Check<Type> {
        match e {
            // (Loc)
            Term::Loc(l) => match self.store.get(*l) {
                Some(t) => Ok(Type::Ref(Rc::new(t.clone()))),
                None => error("Loc", format!("no cell ℓ{l} in μ")),
            },
            // (Fix): `fix f. λx. e` is `let rec f : σ = λx. e in f`
            Term::Fix(f, sig, lam) => {
                self.let_rec(f, sig.as_deref(), lam, &Term::Var(f.clone()), expected)
            }
            Term::Env(theta) => self.env(theta),
            // (Bind)
            Term::Bind(s, e) => {
                self.check(e, &Type::Impl(Row::one(s.clone())))?;
                Ok(Type::Env(Row::one(s.clone())))
            }
            // (Join)
            Term::Join(a, b) => {
                let (a, b) = (self.infer(a)?, self.infer(b)?);
                match (self.resolve(&a), self.resolve(&b)) {
                    (Type::Env(a), Type::Env(b)) => Ok(Type::Env(self.sum(&a, &b)?)),
                    _ => error("Join", "a `join` of what is no `env`"),
                }
            }
            // (With): the services of `θ` are not asked of the surroundings
            Term::With(theta, body) => {
                let Type::Env(given) = self.env(theta)? else {
                    return error("With", "no `env`");
                };
                let (t, inner) = self.own_stage(body, expected)?;
                let (needed, raised) = match inner {
                    Stage::Pure => (Row::none(), Row::none()),
                    Stage::Runtime(r, e) => (r, e),
                };
                let rest = self.minus(&needed, &given)?;
                self.perform(&Stage::Runtime(rest, raised))?;
                Ok(t)
            }
            // the step of `raise`, whose (Errors-Val) is `project`
            Term::Raise(v) => {
                let t = self.constant(&Const::Raise)?;
                self.apply(&t, v)
            }
            // (Handle): the arms of (Try) on what the body raised
            Term::Handle(v, arms) => self.try_(&Term::Raise(v.clone()), arms, expected),
            other => error("Runtime", format!("a form of the run time: {other:?}")),
        }
    }

    /// (Env): each implementation at its service.
    fn env(&mut self, theta: &Env) -> Check<Type> {
        let mut row = Row::none();
        for (s, v) in theta {
            let outer = mem::replace(&mut self.stage, Stage::Pure);
            self.check(v, &Type::Impl(Row::one(s.clone())))?;
            self.stage = outer;
            row.elems.insert(s.clone(), Rc::from([]));
        }
        Ok(Type::Env(row))
    }

    /// (App) on a redex `(λx. e) v` of a configuration: `x` takes the type
    /// of `v`, as the binder whose signature the substitution dropped gave
    /// it. Inferring the λ first would take a parameter it applies for a
    /// pure arrow (Р7), which the source did not.
    pub(super) fn redex(
        &mut self,
        x: &str,
        body: &Term,
        arg: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let p = self.infer(arg)?;
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        self.locals.push((x.into(), mono(p)));
        let r = match expected {
            Some(t) => self.check(body, t).map(|()| t.clone()),
            None => self.infer(body),
        };
        self.locals.pop();
        let inner = mem::replace(&mut self.stage, outer);
        let r = r?;
        self.perform(&inner)?;
        Ok(r)
    }
}
