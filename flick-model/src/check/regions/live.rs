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

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::slice;

use super::super::Checker;
use super::super::generalize::{rigid_inst, substitute, substitute_caps};
use crate::syntax::{Name, Path};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

/// The prefix of a capture row the reading of a parameter's bare data
/// quantifies: no source writes it.
const FRESH: &str = "captures$";

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

/// Places any outer surface hidden qualifier (`Type::Captured`) on a
/// syntactically curried or rank-2 callable starting with a pure arrow onto
/// its inner callable stage after the pure arrows, before instantiation.
fn place_qualifiers(t: &Type) -> Type {
    match t {
        Type::Captured(ty, c) => {
            let ty = place_qualifiers(ty);
            let ty = if starts_with_pure_to_runtime(&ty) {
                push_past_pure_arrows(&ty, c)
            } else {
                ty
            };
            Type::Captured(Rc::new(ty), c.clone())
        }
        Type::Bounded(ty, c) => Type::Bounded(Rc::new(place_qualifiers(ty)), c.clone()),
        Type::Fun(p, s, r) => Type::Fun(
            Rc::new(place_qualifiers(p)),
            s.clone(),
            Rc::new(place_qualifiers(r)),
        ),
        Type::Eff(r, e, a) => Type::Eff(r.clone(), e.clone(), Rc::new(place_qualifiers(a))),
        Type::Forall(s) => Type::Forall(Rc::new(Scheme {
            body: place_qualifiers(&s.body),
            ..(**s).clone()
        })),
        Type::App(a, b) => Type::App(Rc::new(place_qualifiers(a)), Rc::new(place_qualifiers(b))),
        Type::Ref(x) => Type::Ref(Rc::new(place_qualifiers(x))),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(place_qualifiers).collect()),
        Type::Record(f) => Type::Record(Fields {
            fields: f
                .fields
                .iter()
                .map(|(l, t)| (l.clone(), place_qualifiers(t)))
                .collect(),
            tail: f.tail.clone(),
        }),
        other => other.clone(),
    }
}

fn has_runtime_stage(t: &Type) -> bool {
    match t {
        Type::Fun(_, Stage::Runtime(..), _) | Type::Eff(..) => true,
        Type::Fun(_, Stage::Pure, r) => has_runtime_stage(r),
        Type::Forall(s) => has_runtime_stage(&s.body),
        Type::Captured(ty, _) => has_runtime_stage(ty),
        _ => false,
    }
}

fn starts_with_pure_to_runtime(t: &Type) -> bool {
    match t {
        Type::Fun(_, Stage::Pure, r) => has_runtime_stage(r),
        Type::Forall(s) => starts_with_pure_to_runtime(&s.body),
        Type::Captured(ty, _) => starts_with_pure_to_runtime(ty),
        _ => false,
    }
}

fn push_past_pure_arrows(t: &Type, c: &Caps) -> Type {
    match t {
        Type::Fun(p, Stage::Pure, r) => {
            let r = if starts_with_pure_to_runtime(r) {
                push_past_pure_arrows(r, c)
            } else {
                Checker::qualify_hidden((**r).clone(), slice::from_ref(c))
            };
            Type::Fun(p.clone(), Stage::Pure, Rc::new(r))
        }
        Type::Forall(s) => {
            let mentions = |n: &Name| {
                c.atoms
                    .iter()
                    .chain(&c.vars)
                    .any(|v| matches!(v, Var::Named(m) if m == n))
            };
            let mut renamed = HashMap::new();
            let mut used: BTreeSet<Name> = s.vars.iter().map(|(n, _)| n.clone()).collect();
            let vars: Vec<(Name, Kind)> = s
                .vars
                .iter()
                .map(|(n, k)| {
                    if !mentions(n) {
                        return (n.clone(), k.clone());
                    }
                    let mut idx = 0usize;
                    let fresh = loop {
                        let candidate = Name::from(format!("{n}${idx}"));
                        if !used.contains(&candidate) && !mentions(&candidate) {
                            break candidate;
                        }
                        idx += 1;
                    };
                    used.insert(fresh.clone());
                    renamed.insert(n.clone(), rigid_inst(&fresh, k));
                    (fresh, k.clone())
                })
                .collect();
            let body = if renamed.is_empty() {
                s.body.clone()
            } else {
                substitute(&s.body, &renamed)
            };
            let outlives = if renamed.is_empty() {
                s.outlives.clone()
            } else {
                s.outlives
                    .iter()
                    .map(|(cs, r)| (substitute_caps(cs, &renamed), substitute(r, &renamed)))
                    .collect()
            };
            Type::Forall(Rc::new(Scheme {
                body: push_past_pure_arrows(&body, c),
                vars,
                outlives,
            }))
        }
        Type::Captured(ty, inner) => {
            let combined = inner.union(c);
            let ty = push_past_pure_arrows(ty, &combined);
            Type::Captured(Rc::new(ty), inner.clone())
        }
        other => other.clone(),
    }
}

pub(in crate::check) fn arg_caps(t: &Type) -> Caps {
    match t {
        Type::Captured(_, c) => c.clone(),
        _ => Caps::default(),
    }
}

impl Checker<'_> {
    /// `σ` with the `Live` its runtime arrows need, the elision rule, and
    /// the hidden rows of the data it writes bare (`regions.md` §1.4): a
    /// fresh capture row quantified after its variables in each parameter,
    /// and in each result what the parameters before it reach (`elide`), as
    /// `Scheme.elide` of the core (`formal/Formal/Live/Elide.lean`).
    pub(in crate::check) fn live_scheme(&self, s: &Scheme) -> Scheme {
        let mut vars = s.vars.clone();
        let body = place_qualifiers(&self.fresh_rows(&s.body, &mut vars));
        Scheme {
            body: self.elide(
                &body,
                &Caps::default(),
                &regions_of(&vars, &BTreeSet::new()),
            ),
            vars,
            outlives: s.outlives.clone(),
        }
    }

    /// `τ` with the hidden row of each parameter's bare data a fresh
    /// capture row, in order, the parameters of a parameter that is a
    /// function included, as the core's `Ty.fresh`.
    fn fresh_rows(&self, t: &Type, vars: &mut Vec<(Name, Kind)>) -> Type {
        // a written qualifier is a field of the arrow in the core
        let arrow = |t: &Type| match t {
            Type::Captured(x, _) | Type::Bounded(x, _) => matches!(**x, Type::Fun(..)),
            t => matches!(t, Type::Fun(..)),
        };
        match t {
            Type::Captured(x, c) if arrow(t) => {
                Type::Captured(Rc::new(self.fresh_rows(x, vars)), c.clone())
            }
            Type::Bounded(x, c) if arrow(t) => {
                Type::Bounded(Rc::new(self.fresh_rows(x, vars)), c.clone())
            }
            Type::Fun(p, s, r) => {
                let p = match &**p {
                    p if arrow(p) => self.fresh_rows(p, vars),
                    p if self.bare_hidden(p) => {
                        let name = Name::from(format!("{FRESH}{}", vars.len()));
                        vars.push((name.clone(), Kind::Captures));
                        Type::Captured(Rc::new(p.clone()), Caps::var(Var::Named(name)))
                    }
                    p => p.clone(),
                };
                Type::Fun(Rc::new(p), s.clone(), Rc::new(self.fresh_rows(r, vars)))
            }
            t => t.clone(),
        }
    }

    /// What of `c` a stage needs alive: its regions and capture rows. A
    /// type variable is no use of what it stands for: a function generic in
    /// `a` cannot take a value of `a` apart (parametricity, and G4 for
    /// a foreign one).
    fn shown(&self, c: &Caps, regions: &BTreeSet<Name>, bound: &BTreeSet<Name>) -> Caps {
        let free = |v: &&Var| !matches!(v, Var::Named(n) if bound.contains(n));
        Caps {
            atoms: (c.atoms.iter().filter(free))
                .filter(|v| self.is_region(v) || matches!(v, Var::Named(n) if regions.contains(n)))
                .cloned()
                .collect(),
            vars: c.vars.iter().filter(free).cloned().collect(),
        }
    }

    /// `τ` with `Live` of `held` and of what the parameters of each stage
    /// reach in the row of every runtime arrow and `Eff` after them.
    pub(in crate::check) fn elide(&self, t: &Type, held: &Caps, regions: &BTreeSet<Name>) -> Type {
        self.elide_under(t, held, regions, &BTreeSet::new())
    }

    /// [`Self::elide`] under `∀`s whose binders, `bound`, count for nothing
    /// a stage needs or a row holds, as the core's `Caps.under`.
    fn elide_under(
        &self,
        t: &Type,
        held: &Caps,
        regions: &BTreeSet<Name>,
        bound: &BTreeSet<Name>,
    ) -> Type {
        let none = Caps::default();
        let go = |t: &Type, held: &Caps| self.elide_under(t, held, regions, bound);
        match t {
            Type::Fun(p, s, r) => {
                let p = go(p, &none);
                // Nested bounds do not create actual Live requirements, but
                // a signature's own parameter capacity is its conservative
                // allowance, as Core's parameter_captures reads it. Peel only
                // outer qualifiers: a nested forall's capacity stays bound.
                let (mut shown, _) = self.reach_bounded(&p);
                let mut parameter = &p;
                let mut capacity = Caps::default();
                while let Type::Captured(inner, captures) | Type::Bounded(inner, captures) =
                    parameter
                {
                    capacity = capacity.union(captures);
                    parameter = inner;
                }
                if !capacity.is_empty() && !self.holds_nothing(parameter) {
                    shown = shown.union(&capacity);
                }
                let held = held.union(&self.shown(&shown, regions, bound));
                let s = match s {
                    Stage::Runtime(row, e) => Stage::Runtime(add_live(row, &held), e.clone()),
                    Stage::Pure => Stage::Pure,
                };
                // a result's bare data hides what the parameters before it reach
                let r = match go(r, &held) {
                    r if !held.is_empty() && self.bare_hidden(&r) => {
                        Type::Captured(Rc::new(r), held.clone())
                    }
                    r => r,
                };
                Type::Fun(Rc::new(p), s, Rc::new(r))
            }
            Type::Eff(r, e, a) => Type::Eff(add_live(r, held), e.clone(), Rc::new(go(a, &none))),
            Type::Captured(x, c) => Type::Captured(Rc::new(go(x, held)), c.clone()),
            Type::Bounded(x, c) => Type::Bounded(Rc::new(go(x, held)), c.clone()),
            Type::Forall(s) => {
                // a binder named as a name `held` carries is renamed, so the
                // row keeps its meaning under it
                let carried = |n: &Name| {
                    (held.atoms.iter().chain(&held.vars))
                        .any(|v| matches!(v, Var::Named(m) if m == n))
                };
                let mut renamed = HashMap::new();
                let vars: Vec<(Name, Kind)> = (s.vars.iter())
                    .map(|(n, k)| {
                        if !carried(n) {
                            return (n.clone(), k.clone());
                        }
                        let fresh = Name::from(format!("{n}${}", bound.len() + renamed.len()));
                        renamed.insert(n.clone(), rigid_inst(&fresh, k));
                        (fresh, k.clone())
                    })
                    .collect();
                let mut inner = bound.clone();
                inner.extend(vars.iter().map(|(n, _)| n.clone()));
                let body = substitute(&s.body, &renamed);
                Type::Forall(Rc::new(Scheme {
                    body: self.elide_under(&body, held, regions, &inner),
                    vars,
                    outlives: (s.outlives.iter())
                        .map(|(c, r)| (substitute_caps(c, &renamed), substitute(r, &renamed)))
                        .collect(),
                }))
            }
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
