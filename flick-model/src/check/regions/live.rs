//! Regions by use (`docs/formal/regions.md`): a region is live where the
//! row of a stage has `{Live C}` with the region in `C`, the way `Local s`
//! of E8 is. A closure or a task carries no capture qualifier: what it uses
//! of a region shows in its row, and (Scope) already keeps a region out of
//! the rows a `scope` leaves.
//!
//! - A runtime arrow of a signature needs `Live` of what its parameters,
//!   and the parameters of the stages before it, reach (`elide`).
//! - Calling `(τ₁ →R⟨r;e⟩ τ₂)^C` or awaiting `(Eff r e τ)^C` needs `Live C`:
//!   a qualifier on a function or a task is what using it needs alive.
//! - `scope s` discharges `Live` of `s`; work an owner keeps needs every
//!   region of its `Live` to outlive the owner (`Outlives`, §2.4).
//! - `Scope ϱ` of a row keeps `ϱ` live as `{Live {ϱ}}` does, the way Lean
//!   has them as the same row atoms (`scope_live`); `Live ϱ` acquires
//!   nothing into `ϱ` (`regions.md` §6, "Scope keeps its region live").

use std::collections::BTreeSet;
use std::rc::Rc;

use super::super::Checker;
use crate::syntax::{Name, Path};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

/// The variables of kind `Region` of `vars`, with `outer`.
pub(in crate::check) fn regions_of(
    vars: &[(Name, Kind)],
    outer: &BTreeSet<Name>,
) -> BTreeSet<Name> {
    let mut out = outer.clone();
    out.extend(
        vars.iter()
            .filter(|(_, k)| *k == Kind::Region)
            .map(|(n, _)| n.clone()),
    );
    out
}

/// The element `{Live C}` of a row of services.
pub(crate) fn live_service() -> Path {
    Path::new("Prelude", "Live")
}

/// The argument of `{Live C}`: a set of captures carried by a type that
/// hides storage, so ⌈·⌉ of it is `C` and substitution and zonking reach it.
pub(in crate::check) fn live_arg(c: Caps) -> Type {
    let unit = || Type::con("Prelude", "Unit");
    Type::Captured(Rc::new(Type::fun(unit(), Stage::Pure, unit())), c)
}

/// `C` of the argument of `{Live C}`.
/// `ρ + {Live C}`.
pub(in crate::check) fn add_live(r: &Row, c: &Caps) -> Row {
    if c.is_empty() {
        return r.clone();
    }
    let mut out = r.clone();
    let mine = out
        .elems
        .get(&live_service())
        .map(|a| arg_caps(&a[0]))
        .unwrap_or_default();
    out.elems
        .insert(live_service(), Rc::from([live_arg(mine.union(c))]));
    out
}

/// `φ` with `Live C` in its row; a pure stage stays pure.
pub(in crate::check) fn live_stage(s: &Stage, c: &Caps) -> Stage {
    match s {
        Stage::Runtime(r, e) => Stage::Runtime(add_live(r, c), e.clone()),
        Stage::Pure => Stage::Pure,
    }
}

/// `r` without `Live` of nothing: once its qualifier is known, it needs
/// nothing.
pub(in crate::check) fn drop_empty_live(r: &mut Row) {
    if r.elems
        .get(&live_service())
        .is_some_and(|a| arg_caps(&a[0]).is_empty())
    {
        r.elems.remove(&live_service());
    }
}

pub(in crate::check) fn arg_caps(t: &Type) -> Caps {
    match t {
        Type::Captured(_, c) => c.clone(),
        _ => Caps::default(),
    }
}

impl Checker<'_> {
    /// `σ` with the `Live` its runtime arrows need: the elision rule.
    pub(in crate::check) fn live_scheme(&self, s: &Scheme) -> Scheme {
        Scheme {
            vars: s.vars.clone(),
            body: self.elide(
                &s.body,
                &Caps::default(),
                &regions_of(&s.vars, &BTreeSet::new()),
            ),
            outlives: s.outlives.clone(),
        }
    }

    /// What of `c` a stage needs alive: its regions and capture rows. A
    /// type variable is no use of what it stands for: a function generic in
    /// `a` cannot take a value of `a` apart (parametricity, and G4 for
    /// a foreign one).
    fn shown(&self, c: &Caps, regions: &BTreeSet<Name>) -> Caps {
        Caps {
            atoms: c
                .atoms
                .iter()
                .filter(|v| self.is_region(v) || matches!(v, Var::Named(n) if regions.contains(n)))
                .cloned()
                .collect(),
            vars: c.vars.clone(),
        }
    }

    /// `τ` with `Live` of `held` and of what the parameters of each stage
    /// reach in the row of every runtime arrow and `Eff` after them.
    pub(in crate::check) fn elide(&self, t: &Type, held: &Caps, regions: &BTreeSet<Name>) -> Type {
        let none = Caps::default();
        let go = |t: &Type, held: &Caps| self.elide(t, held, regions);
        match t {
            Type::Fun(p, s, r) => {
                let p = go(p, &none);
                let held = held.union(&self.shown(&self.reach(&p), regions));
                let s = match s {
                    Stage::Runtime(row, e) => Stage::Runtime(add_live(row, &held), e.clone()),
                    Stage::Pure => Stage::Pure,
                };
                let r = go(r, &held);
                Type::Fun(Rc::new(p), s, Rc::new(r))
            }
            Type::Eff(r, e, a) => Type::Eff(add_live(r, held), e.clone(), Rc::new(go(a, &none))),
            Type::Captured(x, c) => Type::Captured(Rc::new(go(x, held)), c.clone()),
            Type::Bounded(x, c) => Type::Bounded(Rc::new(go(x, held)), c.clone()),
            Type::Forall(s) => Type::Forall(Rc::new(Scheme {
                vars: s.vars.clone(),
                body: self.elide(&s.body, held, &regions_of(&s.vars, regions)),
                outlives: s.outlives.clone(),
            })),
            Type::App(a, b) => Type::App(Rc::new(go(a, &none)), Rc::new(go(b, &none))),
            Type::Ref(x) => Type::Ref(Rc::new(go(x, &none))),
            Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| go(t, &none)).collect()),
            Type::Record(f) => Type::Record(Fields {
                fields: f
                    .fields
                    .iter()
                    .map(|(l, t)| (l.clone(), go(t, &none)))
                    .collect(),
                tail: f.tail.clone(),
            }),
            other => other.clone(),
        }
    }

    /// The end of `scope s`: its region is live no further. What still
    /// waits to be known of a qualifier is grown first, so a value acquired
    /// in the `scope` shows its region here.
    pub(in crate::check) fn discharge_live(&self, r: &mut Row, s: &Name) {
        let Some(args) = r.elems.get(&live_service()) else {
            return;
        };
        let mut c = self.zonk_caps(&arg_caps(&args[0]));
        c.atoms.remove(&Var::Named(s.clone()));
        if c.is_empty() {
            r.elems.remove(&live_service());
        } else {
            r.elems.insert(live_service(), Rc::from([live_arg(c)]));
        }
    }

    /// The regions `Scope ϱ` of `r` keeps live: `Live C` and `Scope s`
    /// are the same row atoms (`formal/Formal/Live/Mode.lean`,
    /// `Row.scope s = Row.ofCaps s.reach`), so `{Scope ϱ}` has `{Live {ϱ}}`
    /// where `Live` is checked within the row (`rows.rs` `within`). Not the
    /// other way: `Live ϱ` acquires nothing into `ϱ`.
    ///
    /// Every other service indexed by a region keeps it live too, as `Setup
    /// ϱ` (`Row.err E U` has the atoms of `U`): a handler of one needs what
    /// indexes it live where it is installed (`indexed_live`).
    pub(in crate::check) fn scope_live(&self, r: &Row) -> Caps {
        let mut out = Caps::default();
        for (service, args) in &r.elems {
            if *service != live_service() {
                out = out.union(&self.indexed_live(args));
            }
        }
        out
    }

    /// What the parameters `args` of a service reach: what a handler of it
    /// needs live where it is installed, and what its entry keeps live.
    pub(in crate::check) fn indexed_live(&self, args: &[Type]) -> Caps {
        args.iter()
            .fold(Caps::default(), |out, t| out.union(&self.reach(t)))
    }

    /// Work an owner keeps (`Fiber.Provided R ϱ`): each region `R` needs
    /// alive outlives `ϱ`, decided at the end with `Outlives` (§2.4). Its
    /// `Scope` is the owner's own, `ϱ` (children acquire in the region of
    /// their owner), which `ϱ` outlives: `scope_live` adds nothing here.
    pub(in crate::check) fn keep_live(&mut self, wanted: &Row, owner: &Type) {
        if let Some(args) = wanted.elems.get(&live_service()) {
            let facts = self.regions.facts.clone();
            self.regions
                .needs
                .push((arg_caps(&args[0]), owner.clone(), facts));
        }
    }
}
