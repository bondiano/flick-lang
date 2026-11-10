//! The forms of the run time (`docs/formal/core.md` §3.6): what a
//! configuration `⟨μ, E[e]⟩` holds besides source forms, typed so that
//! Preservation can be checked on every state the machine passes through
//! (the model properties). `Ψ` is a meta per cell, made before anything else is checked, so
//! `Gen` never quantifies a cell's type (T3). The forms of regions and
//! `⊢ Λ` are those of `docs/formal/regions.md` §3.4.

use std::collections::HashMap;
use std::mem;
use std::rc::Rc;

use super::expr::mono;
use super::regions::{arg_caps, live_arg, live_service, owner};
use super::{Check, Checker, Inst, error, substitute};
use crate::decls::Decls;
use crate::eval::Lambda;
use crate::eval::regions::{Finalizer, reach, region_name};
use crate::subst::any;
use crate::syntax::{Const, Env, Term};
use crate::types::{Kind, Row, Stage, Type, Var};

/// `·;Ψ;Λ ⊢ e : τ ! φ` for some `Ψ` with `⊢ μ : Ψ`, and `⊢ Λ`: a
/// configuration of the machine against the type of the program. Returns
/// the least `φ`: a row still unknown once everything is settled is
/// `none` (§2.1).
pub fn check_config(
    decls: &Decls,
    mem: &[Term],
    lambda: &Lambda,
    e: &Term,
    expected: &Type,
) -> Check<Stage> {
    let mut c = Checker::new(decls);
    c.config = true;
    for (pi, r) in lambda.regions.iter().enumerate() {
        c.declare_region(region_name(pi), r.parent.map(region_name));
    }
    c.store = (0..mem.len()).map(|_| c.fresh()).collect();
    for (l, v) in mem.iter().enumerate() {
        let t = c.store[l].clone();
        let outer = mem::replace(&mut c.stage, Stage::Pure);
        c.check(v, &t)?;
        if mem::replace(&mut c.stage, outer) != Stage::Pure {
            return error("μ", format!("the value in ℓ{l} is not pure"));
        }
    }
    if any(
        e,
        &|t| matches!(t, Term::Inside(pi, ..) if !lambda.regions[*pi].live),
    ) {
        return error("Inside", "the body of a closed region runs");
    }
    c.check(e, expected)?;
    c.lambda(lambda, mem)?;
    c.finish()?;
    Ok(match c.zonk_stage(&c.stage.clone()) {
        Stage::Runtime(r, e) => Stage::Runtime(least(r), least(e)),
        Stage::Pure => Stage::Pure,
    })
}

/// The least solution of what is still unknown: a row or a capture
/// variable never solved stands for nothing, also in `Live`.
fn least(mut r: Row) -> Row {
    r.vars.retain(|v| !matches!(v, Var::Meta(_)));
    if let Some(args) = r.elems.get(&live_service()) {
        let mut c = arg_caps(&args[0]);
        c.atoms.retain(|v| !matches!(v, Var::Meta(_)));
        c.vars.retain(|v| !matches!(v, Var::Meta(_)));
        if c.is_empty() {
            r.elems.remove(&live_service());
        } else {
            r.elems.insert(live_service(), Rc::from([live_arg(c)]));
        }
    }
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
            Term::Fix(f, sig, lam) => self.fix(f, sig.as_deref(), lam, expected),
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
                let (mut needed, raised) = match inner {
                    Stage::Pure => (Row::none(), Row::none()),
                    Stage::Runtime(r, e) => (self.zonk_row(&r), e),
                };
                self.settle_provide(&mut needed, &given, None)?;
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
            // (Own)
            Term::Own(pi) => Ok(owner(&region_name(*pi))),
            // (Cap): what the binding gives, of region `π`
            Term::Cap(p, pi, _) => {
                let Some(s) = self.decls.values.get(p).cloned() else {
                    return error("Cap", format!("no type for `{p}`"));
                };
                let map: HashMap<_, _> = s
                    .vars
                    .iter()
                    .map(|(v, k)| {
                        let inst = match (k, pi) {
                            (Kind::Region, Some(pi)) => {
                                Inst::Type(Type::Var(Var::Named(region_name(*pi))))
                            }
                            _ => self.fresh_inst(k),
                        };
                        (v.clone(), inst)
                    })
                    .collect();
                let mut t = substitute(&s.body, &map);
                while let Type::Fun(_, _, r) = t {
                    t = (*r).clone();
                }
                Ok(t)
            }
            Term::Inside(pi, s, body) => self.inside(s, region_name(*pi), body, expected),
            other => error("Runtime", format!("a form of the run time: {other:?}")),
        }
    }

    /// `⊢ Λ`: the children and finalizers of each live region are typed as
    /// what runs in it, and reach only regions that outlive it (§2.4, T4).
    fn lambda(&mut self, lambda: &Lambda, mem: &[Term]) -> Check<()> {
        for (pi, r) in lambda.regions.iter().enumerate().filter(|(_, r)| r.live) {
            let fins = r.finalizers.iter().filter_map(|f| match f {
                Finalizer::Run(t, _) => Some(t),
                Finalizer::Release(_) => None,
            });
            let kids = r.children.iter().flat_map(|(f, t, _)| [f, t]);
            let outer = lambda.outlived_by(pi);
            // made one in another, so in the order they were made
            let enclosing: Vec<_> = outer.iter().map(|p| region_name(*p)).collect();
            for t in fins.chain(kids) {
                let stage = mem::replace(&mut self.stage, Stage::Pure);
                self.nested_in(enclosing.clone(), |c| c.infer(t))?;
                self.stage = stage;
                if let Some(q) = reach(t, mem).difference(&outer).next() {
                    return error(
                        "Λ",
                        format!("work of region π{pi} reaches π{q}, which may close first"),
                    );
                }
            }
        }
        Ok(())
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
    /// pure arrow (`docs/formal/core.md#a-callee-of-unknown-type-is-a-pure-arrow`), which the source did not.
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
