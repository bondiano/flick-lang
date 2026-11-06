//! (Match), (Task), (Await), (Try) and (Provide) of
//! `docs/formal/core.md` §2.3, and the compiler-owned dictionaries of
//! records and tuples (§1.6).

use std::iter;
use std::mem;
use std::rc::Rc;

use super::expr::mono;
use super::{Check, Checker, error};
use crate::subst::matches;
use crate::syntax::{Arm, Name, Pat, Term};
use crate::types::{Row, Stage, Type};

impl Checker<'_> {
    /// (Match): every arm at one type; the arms without `when` cover `τ`.
    pub(super) fn match_(
        &mut self,
        e: &Term,
        arms: &[Arm],
        expected: Option<&Type>,
    ) -> Check<Type> {
        let t = self.infer(e)?;
        let result = match expected {
            Some(r) => r.clone(),
            None => self.fresh(),
        };
        // in a configuration, (Match) may put the arms at the type of any
        // one of them: a λ a signature checked before the step took its
        // arrow from it, and inferred takes its least one
        let firsts = if self.config && expected.is_none() {
            arms.len()
        } else {
            1
        };
        let mut checked = Ok(());
        for first in 0..firsts {
            let saved = (first + 1 < firsts).then(|| self.clone());
            let order = iter::once(first).chain((0..arms.len()).filter(|&i| i != first));
            checked = order
                .filter_map(|i| arms.get(i))
                .try_for_each(|arm| self.arm(arm, &t, &result));
            match saved {
                Some(saved) if checked.is_err() => *self = saved,
                _ => break,
            }
        }
        checked?;
        let unguarded: Vec<&Pat> = arms
            .iter()
            .filter(|a| a.guard.is_none())
            .map(|a| &a.pat)
            .collect();
        // (Match-Val): in a configuration, the rest of a `match` after a
        // guard that failed covers the value it holds, not its type
        let covers_value =
            self.config && e.is_value() && unguarded.iter().any(|p| matches(p, e, &mut Vec::new()));
        if !covers_value && !self.exhaustive(&t, &unguarded)? {
            return error(
                "Match",
                format!(
                    "the patterns do not cover every value of `{}` (Exh)",
                    self.zonk(&t)
                ),
            );
        }
        Ok(result)
    }

    /// `⊢ p : θ ⇒ Δ`, `Γ, Δ ⊢ g : Bool`, `Γ, Δ ⊢ e : τ'`.
    fn arm(&mut self, arm: &Arm, scrutinee: &Type, result: &Type) -> Check<()> {
        // what the arm learns of the rigid variables holds in it only
        let learned = self.equalities.len();
        let checked = self.arm_under(arm, scrutinee, result);
        self.equalities.truncate(learned);
        checked
    }

    fn arm_under(&mut self, arm: &Arm, scrutinee: &Type, result: &Type) -> Check<()> {
        let bound = self.pattern(&arm.pat, scrutinee)?;
        let depth = self.locals.len();
        self.locals
            .extend(bound.into_iter().map(|(x, t)| (x, mono(t))));
        let checked = (|| {
            if let Some(g) = &arm.guard {
                self.check(g, &Type::con("Prelude", "Bool"))?;
            }
            self.check(&arm.body, result)
        })();
        self.locals.truncate(depth);
        checked
    }

    /// `e` on a stage of its own: what (Task), (Try) and (Provide) check
    /// their bodies on. Returns the type and that stage.
    pub(super) fn own_stage(&mut self, e: &Term, expected: Option<&Type>) -> Check<(Type, Stage)> {
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        let t = match expected {
            Some(t) => self.check(e, t).map(|()| t.clone()),
            None => self.infer(e),
        };
        let inner = mem::replace(&mut self.stage, outer);
        Ok((t?, inner))
    }

    /// (Task): `task e : (Eff r ε τ)^C ! P` with `φ ⊑ R⟨r; ε⟩`, and what
    /// `e` uses of `Γ` within `⌈(Eff r ε τ)^C⌉` (`regions.md` §2.3).
    pub(super) fn task(&mut self, body: &Term, expected: Option<&Type>) -> Check<Type> {
        let qualifier = expected.map(|t| self.qualifier(t));
        let wanted = expected.map(|t| self.resolve(t));
        let result = match &wanted {
            Some(Type::Eff(_, _, a)) => Some((**a).clone()),
            _ => None,
        };
        let (t, inner) = self.own_stage(body, result.as_ref())?;
        let (r, e) = match &wanted {
            Some(Type::Eff(r, e, _)) => (r.clone(), e.clone()),
            _ => (self.fresh_row(), self.fresh_row()),
        };
        self.stage_within(&inner, &Stage::Runtime(r.clone(), e.clone()))?;
        let task = Term::Task(Rc::new(body.clone()));
        Ok(self.close_over(&task, Type::Eff(r, e, Rc::new(t)), qualifier))
    }

    /// (Await): `φ ⊔ R⟨r; ε⟩`, one layer of `Eff` off.
    pub(super) fn await_(&mut self, e: &Term) -> Check<Type> {
        let t = self.infer(e)?;
        let (r, x, a) = match self.resolve(&t) {
            Type::Eff(r, x, a) => (r, x, (*a).clone()),
            other => {
                let (r, x, a) = (self.fresh_row(), self.fresh_row(), self.fresh());
                self.unify(&other, &Type::Eff(r.clone(), x.clone(), Rc::new(a.clone())))?;
                (r, x, a)
            }
        };
        self.perform(&Stage::Runtime(r, x))?;
        Ok(a)
    }

    /// (Try) and (Try-Pure): the body raises `ε_b`; an arm `E p̄` takes
    /// `E ῡ`, an arm `x` or `_` takes `Errors ε_b`; what no arm without
    /// `when` covers (`H`) stays in the row. The arms run outside the `try`.
    pub(super) fn try_(
        &mut self,
        body: &Term,
        arms: &[Arm],
        expected: Option<&Type>,
    ) -> Check<Type> {
        let (t, inner) = self.own_stage(body, expected)?;
        let (services, raised) = match &inner {
            Stage::Pure => (Row::none(), Row::none()),
            Stage::Runtime(r, e) => (r.clone(), e.clone()),
        };
        let mut handled = Row::none();
        let mut all = false;
        for arm in arms {
            let theta = match strip(&arm.pat) {
                Pat::Var(_) | Pat::Wild => {
                    all |= arm.guard.is_none();
                    Type::Errors(raised.clone())
                }
                Pat::Con(c, _) if self.decls.errors.contains(&c.ty) => {
                    let ctor = self.constructor(&c.ty.module, &c.name)?;
                    let e = result_of(ctor);
                    if arm.guard.is_none()
                        && self.exhaustive(&e, &[&arm.pat])?
                        && let Some((name, args)) = self.error_of(&e)
                    {
                        handled.elems.insert(name, args);
                    }
                    e
                }
                _ => return error("Try", "a handler pattern is not an error (`FLICK-E007`)"),
            };
            self.arm(arm, &theta, &t)?;
        }
        if inner == Stage::Pure {
            return Ok(t);
        }
        let rest = if all {
            Row::none()
        } else {
            self.minus(&raised, &handled)?
        };
        self.perform(&Stage::Runtime(services, rest))?;
        Ok(t)
    }

    /// (Provide): the layer is built around the body, whose services it
    /// does not give pass outwards; `R⟨rᵢ; εₗ⟩` is always a step.
    pub(super) fn provide(
        &mut self,
        layer: &Term,
        body: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let l = self.infer(layer)?;
        let (inputs, errors, outputs) = match self.resolve(&l) {
            Type::Layer(i, e, o) => (i, e, o),
            // (Provide-Env): the layer is built, its steps are in `φₗ`
            Type::Env(o) => (Row::none(), Row::none(), o),
            other => {
                let (i, e, o) = (self.fresh_row(), self.fresh_row(), self.fresh_row());
                self.unify(&other, &Type::Layer(i.clone(), e.clone(), o.clone()))?;
                (i, e, o)
            }
        };
        let p = self.enter_provide();
        let checked = self.own_stage(body, expected);
        self.leave_provide();
        let (t, inner) = checked?;
        let (mut needed, raised) = match inner {
            Stage::Pure => (Row::none(), Row::none()),
            Stage::Runtime(r, e) => (self.zonk_row(&r), e),
        };
        self.settle_provide(&mut needed, &self.zonk_row(&outputs), Some(&p))?;
        let rest = self.minus(&needed, &outputs)?;
        let services = self.sum(&inputs, &rest)?;
        let errors = self.sum(&errors, &raised)?;
        self.perform(&Stage::Runtime(services, errors))?;
        Ok(t)
    }

    /// The compiler-owned instance of `class` for the record or tuple `t`
    /// is in `C.Dict t`: each field's dictionary is at that field's type.
    pub(super) fn structural_fields(
        &mut self,
        class: &Name,
        fields: &[Term],
        t: &Type,
    ) -> Check<()> {
        let Type::Dict(c, args) = self.zonk(t) else {
            return error("Structural", "not a dictionary");
        };
        let at: Vec<Type> = match args.first().map(|a| self.zonk(a)) {
            Some(Type::Record(f)) if f.tail.is_none() => f.fields.values().cloned().collect(),
            Some(Type::Tuple(ts)) => ts.to_vec(),
            other => {
                return error(
                    "Structural",
                    format!(
                        "`{class}` of {} is no record or tuple",
                        other.map_or("?".into(), |t| t.to_string())
                    ),
                );
            }
        };
        if at.len() != fields.len() {
            return error("Structural", "one dictionary per field");
        }
        for (d, field) in fields.iter().zip(at) {
            self.check(d, &Type::Dict(c.clone(), Rc::from([field])))?;
        }
        Ok(())
    }
}

/// The pattern under the alternatives and names of an or-pattern.
fn strip(p: &Pat) -> &Pat {
    match p {
        Pat::Or(ps) => ps.first().map_or(p, strip),
        _ => p,
    }
}

/// `T ᾱ` of `τ₁ →P … →P T ᾱ`.
fn result_of(mut t: Type) -> Type {
    while let Type::Fun(_, _, r) = t {
        t = (*r).clone();
    }
    t
}
