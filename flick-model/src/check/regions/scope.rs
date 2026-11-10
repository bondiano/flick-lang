//! (Scope) (`docs/formal/regions.md` §2.3): a fresh region, its owner,
//! the stage of the body, and the regions that must not leave it; the
//! owner a step acquires into (§1.4), and the errors (Err-Raise) checks.

use std::collections::{BTreeSet, HashMap};
use std::mem;
use std::rc::Rc;

use super::super::expr::mono;
use super::super::generalize::substitute_caps;
use super::super::{Check, Checker, Inst, error, substitute};
use super::{owner, scope_service};
use crate::syntax::{Name, Term};
use crate::types::{Caps, Row, Scheme, Stage, Type, Var};

impl Checker<'_> {
    /// (Scope): `s` fresh one level deeper, `s : Owner s` in the body, the
    /// body's stage within `R⟨{Scope s} + r; ε⟩`, and `s` nowhere in the
    /// type and the rows the `scope` leaves.
    pub(in crate::check) fn scope(
        &mut self,
        name: &Name,
        body: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        self.scoped(name, None, body, expected)
    }

    /// (Inside) (§3.4): (Scope) with the region `π` of `Λ`, which
    /// `declare_region` made.
    pub(in crate::check) fn inside(
        &mut self,
        name: &Name,
        pi: Name,
        body: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        self.scoped(name, Some(pi), body, expected)
    }

    /// Region `π` of `Λ`, made in `parent`: every one is declared, the
    /// closed ones too, whose cells stay in `μ`.
    pub(in crate::check) fn declare_region(&mut self, pi: Name, parent: Option<Name>) {
        self.regions.kinds.insert(Var::Named(pi.clone()));
        self.regions.scopes.insert(pi, (0, parent));
    }

    fn scoped(
        &mut self,
        name: &Name,
        pi: Option<Name>,
        body: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        self.level += 1;
        let s: Name = match pi {
            Some(pi) => pi,
            // a region is fresh: one a `scope` around binds is renamed
            None if self.regions.scopes.contains_key(name) => {
                self.rigid += 1;
                format!("{name}'{}", self.rigid).into()
            }
            None => name.clone(),
        };
        let region = Var::Named(s.clone());
        if !self.regions.scopes.contains_key(&s) {
            let parent = self.regions.nest.last().cloned();
            self.regions.scopes.insert(s.clone(), (self.level, parent));
            self.regions.kinds.insert(region.clone());
        }
        self.regions.open.push(s.clone());
        self.regions.nest.push(s.clone());
        self.regions.renamed.push((name.clone(), s.clone()));
        self.locals.push((name.clone(), mono(owner(&s))));
        let checked = self.own_stage(body, expected);
        self.locals.pop();
        self.regions.renamed.pop();
        self.regions.nest.pop();
        self.regions.open.pop();
        self.level -= 1;
        let s = &s;
        let (t, inner) = checked?;
        self.grow_holds()?;
        let (mut r, e) = match inner {
            Stage::Pure => (Row::none(), Row::none()),
            Stage::Runtime(r, e) => (self.zonk_row(&r), self.zonk_row(&e)),
        };
        self.settle_scope(&mut r, s);
        self.discharge_live(&mut r, s);
        if let Some(args) = r.elems.remove(&scope_service()) {
            for a in args.iter() {
                self.unify(a, &Type::Var(region.clone()))?;
            }
        }
        let left = Type::Tuple(Rc::from([
            t.clone(),
            Type::Errors(self.zonk_row(&r)),
            Type::Errors(self.zonk_row(&e)),
        ]));
        if self.names(&left).contains(s) {
            return error(
                "Scope",
                format!(
                    "region `{s}` leaves its `scope` in `{}` (`FLICK-G002`)",
                    self.zonk(&left)
                ),
            );
        }
        self.lower(&left, self.level);
        self.perform(&Stage::Runtime(r, e))?;
        Ok(t)
    }

    /// A signature written in the body of `scope`s: the regions it names
    /// are theirs.
    pub(in crate::check) fn in_scopes(&self, s: &Scheme) -> Scheme {
        let map: HashMap<Name, Inst> = self
            .regions
            .renamed
            .iter()
            .filter(|(n, r)| n != r && !s.vars.iter().any(|(v, _)| v == n))
            .map(|(n, r)| (n.clone(), Inst::Type(Type::Var(Var::Named(r.clone())))))
            .collect();
        if map.is_empty() {
            return s.clone();
        }
        Scheme {
            vars: s.vars.clone(),
            body: substitute(&s.body, &map),
            outlives: s
                .outlives
                .iter()
                .map(|(c, r)| {
                    let c = substitute_caps(c, &map);
                    (c, substitute(r, &map))
                })
                .collect(),
        }
    }

    /// Whether a value at `level` may take what `t` names: no region of a
    /// `scope` deeper than it (§2.3 (Scope), §2.6).
    pub(in crate::check) fn escapes(&self, t: &Type, level: u32) -> Check<()> {
        for n in self.names(t) {
            if let Some((l, _)) = self.regions.scopes.get(&n)
                && *l > level
            {
                return error(
                    "Scope",
                    format!("region `{n}` would leave its `scope` (`FLICK-G002`)"),
                );
            }
        }
        Ok(())
    }

    /// The named variables of `t`, in its rows and qualifiers too.
    fn names(&self, t: &Type) -> BTreeSet<Name> {
        let mut out = BTreeSet::new();
        names_into(&self.zonk(t), &mut out);
        out
    }

    /// A step that needs `Scope ϱ`, `ϱ` not known yet, acquires into the
    /// innermost `scope` (§1.4). Outside the `scope`s of its λ or task, into
    /// the region the stage of that names (§6 "Leases instead of early
    /// close"), else in a task argument once its call is checked, if no
    /// later argument names `ϱ` (§6 "Scope region of a task argument is
    /// resolved after the call").
    pub(in crate::check) fn lexical_owner(&mut self, s: &Stage) -> Check<()> {
        let (Stage::Runtime(r, _), Some(inner)) = (s, self.regions.open.last().cloned()) else {
            return Ok(());
        };
        let Some(args) = self.zonk_row(r).elems.get(&scope_service()).cloned() else {
            return Ok(());
        };
        for a in args.iter() {
            if !matches!(self.follow(a), Type::Var(Var::Meta(_))) {
                continue;
            }
            let here = self.regions.open.len();
            match (self.regions.declared.clone(), self.regions.task_argument) {
                (Some((declared, open)), _) if open == here => self.unify(a, &declared)?,
                (_, Some((call, open))) if open == here => {
                    self.regions.calls[call].push((a.clone(), inner.clone()));
                }
                _ => self.unify(a, &Type::Var(Var::Named(inner.clone())))?,
            }
        }
        Ok(())
    }

    /// `check` of a call, then the regions its task arguments left open.
    pub(in crate::check) fn call(
        &mut self,
        check: impl FnOnce(&mut Self) -> Check<Type>,
    ) -> Check<Type> {
        self.regions.calls.push(Vec::new());
        let t = check(self);
        let waiting = self.regions.calls.pop().unwrap_or_default();
        let t = t?;
        for (a, inner) in waiting {
            if matches!(self.follow(&a), Type::Var(Var::Meta(_))) {
                self.unify(&a, &Type::Var(Var::Named(inner)))?;
            }
        }
        Ok(t)
    }

    /// `check` of the body of a λ or a task at `stage`, its declared one.
    pub(in crate::check) fn declaring<T>(
        &mut self,
        stage: Option<&Stage>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let declared = match stage {
            Some(Stage::Runtime(r, _)) => self.zonk_row(r).elems.get(&scope_service()).cloned(),
            _ => None,
        };
        let declared = match declared.as_deref() {
            Some([region]) if !matches!(self.follow(region), Type::Var(Var::Meta(_))) => {
                Some((region.clone(), self.regions.open.len()))
            }
            _ => None,
        };
        let outer = mem::replace(&mut self.regions.declared, declared);
        let out = check(self);
        self.regions.declared = outer;
        out
    }

    /// `check` of an argument of type `Eff` of the innermost call.
    pub(in crate::check) fn task_argument(
        &mut self,
        check: impl FnOnce(&mut Self) -> Check<()>,
    ) -> Check<()> {
        let Some(call) = self.regions.calls.len().checked_sub(1) else {
            return check(self);
        };
        let here = Some((call, self.regions.open.len()));
        let outer = mem::replace(&mut self.regions.task_argument, here);
        let checked = check(self);
        self.regions.task_argument = outer;
        checked
    }

    /// (Err-Raise) is recorded with the step: the errors of a step under
    /// a `scope` reach none of its region.
    pub(in crate::check) fn raised(&mut self, s: &Stage) {
        if let Stage::Runtime(_, e) = s
            && !self.regions.open.is_empty()
            && !e.is_none()
        {
            let open = self.regions.open.clone();
            self.regions.raised.push((e.clone(), open));
        }
    }
}

fn names_into(t: &Type, out: &mut BTreeSet<Name>) {
    let row = |r: &Row, out: &mut BTreeSet<Name>| {
        for args in r.elems.values() {
            args.iter().for_each(|a| names_into(a, out));
        }
        for v in &r.vars {
            if let Var::Named(n) = v {
                out.insert(n.clone());
            }
        }
    };
    let caps = |c: &Caps, out: &mut BTreeSet<Name>| {
        for v in c.atoms.iter().chain(&c.vars) {
            if let Var::Named(n) = v {
                out.insert(n.clone());
            }
        }
    };
    match t {
        Type::Var(Var::Named(n)) => {
            out.insert(n.clone());
        }
        Type::Var(_) | Type::Con(_) => {}
        Type::App(a, b) => {
            names_into(a, out);
            names_into(b, out);
        }
        Type::Fun(p, s, r) => {
            names_into(p, out);
            if let Stage::Runtime(a, b) = s {
                row(a, out);
                row(b, out);
            }
            names_into(r, out);
        }
        Type::Eff(a, b, x) => {
            row(a, out);
            row(b, out);
            names_into(x, out);
        }
        Type::EffOf(a, b) => {
            row(a, out);
            row(b, out);
        }
        Type::Ref(x) => names_into(x, out),
        Type::Layer(a, b, c) => [a, b, c].iter().for_each(|r| row(r, out)),
        Type::Record(f) => f.fields.values().for_each(|t| names_into(t, out)),
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().for_each(|t| names_into(t, out)),
        Type::Errors(r) | Type::Impl(r) | Type::Tag(r) | Type::Env(r) => row(r, out),
        Type::Forall(s) => names_into(&s.body, out),
        Type::Captured(x, c) | Type::Bounded(x, c) => {
            caps(c, out);
            names_into(x, out);
        }
    }
}
