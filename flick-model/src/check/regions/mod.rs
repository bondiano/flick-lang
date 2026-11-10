//! Regions and captures (`docs/formal/regions.md` §2), written from the
//! document and not from flick-core's `types/regions/`: ⌈·⌉ (§2.1), the
//! obligations of (≤-Cap), (Lam), (Task), (Con) and (Inst) and their least
//! solution (§2.6), (Scope) (§2.3), `Outlives` (§2.4) and (Err-Raise)
//! (§2.5). An obligation waits for the end of the declaration, when every
//! type it names is known: a capture row grows to the least set its
//! obligations need, and a meta outside a `scope` never takes its region.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::{Check, Checker, error};

mod live;
mod provided;
mod scope;
mod solve;
use crate::syntax::{Con, Name, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};
pub(crate) use live::live_service;
pub(in crate::check) use live::{
    add_live, arg_caps, drop_empty_live, live_arg, live_stage, regions_of,
};

/// `⋃ ⌈held⌉ ⊆ ⋃ ⌈within⌉ ∪ allowed`.
#[derive(Clone, Debug)]
pub(super) struct Hold {
    held: Vec<Type>,
    within: Vec<Type>,
    allowed: Caps,
    rule: &'static str,
}

/// `Outlives C ϱ` and the facts in force where it is needed.
type Need = (Caps, Type, Vec<(Caps, Type)>);

/// What the rules of regions keep while a declaration is checked.
#[derive(Clone, Default)]
pub(super) struct Regions {
    holds: Vec<Hold>,
    /// Each region a `scope` of the declaration binds: its level and the
    /// region of the `scope` around it.
    scopes: HashMap<Name, (u32, Option<Name>)>,
    /// The `scope`s around the position being checked, innermost last.
    open: Vec<Name>,
    /// The variables of kind `Region`: a signature's and metas.
    pub(super) kinds: HashSet<Var>,
    /// The errors of each step under a `scope`, with the `scope`s open.
    raised: Vec<(Row, Vec<Name>)>,
    /// `Outlives C ϱ` an instance needs, with the facts `Θ` in force there.
    needs: Vec<Need>,
    /// `Θ` of the signatures around the position being checked.
    pub(super) facts: Vec<(Caps, Type)>,
    /// The region each `scope` around binds its name to, innermost last.
    renamed: Vec<(Name, Name)>,
    /// The regions of the `scope`s and `provide`s around, innermost last.
    nest: Vec<Name>,
    /// `Provided R ϱ` of the signatures around (§2.7).
    givens: Vec<(Row, Type)>,
    /// The calls being checked, innermost last: the unknown `Scope`
    /// regions of steps of their task arguments, each with the `scope`
    /// innermost there.
    calls: Vec<Vec<(Type, Name)>>,
    /// While an argument of type `Eff` is checked: its call, and how many
    /// `scope`s are open there.
    task_argument: Option<(usize, usize)>,
    /// The region of `Scope` the stage of the λ or task being checked
    /// names, once known, with how many `scope`s are open there.
    declared: Option<(Type, usize)>,
}

/// The element `Scope ϱ` of a row of services.
pub(crate) fn scope_service() -> Path {
    Path::new("Prelude", "Scope")
}

pub(super) fn owner(s: &Name) -> Type {
    Type::app(
        Type::con("Fiber", "Owner"),
        Type::Var(Var::Named(s.clone())),
    )
}

const SCALARS: [&str; 7] = ["Int", "Float", "BigInt", "String", "Bool", "Unit", "Never"];

impl Checker<'_> {
    /// `⌈τ⌉` (§2.1): the atoms a value of `t` may reach, an atom `α^{≤C}`
    /// as `C`.
    pub(super) fn reach(&self, t: &Type) -> Caps {
        let (mut out, bounded) = self.reach_bounded(t);
        for (_, c) in bounded {
            out = out.union(&c);
        }
        out
    }

    /// `⌈τ⌉` with each atom `α^{≤C}` apart (`docs/formal/regions.md#a-bounded-variable-reaches-what-the-variable-reaches`): a type variable under a
    /// bound reaches what `α` does, no more than `C`.
    pub(super) fn reach_bounded(&self, t: &Type) -> (Caps, Vec<(Var, Caps)>) {
        let mut out = Caps::default();
        let mut bounded = Vec::new();
        self.reach_into(&self.zonk(t), &mut out, &mut bounded);
        (out, bounded)
    }

    fn reach_into(&self, t: &Type, out: &mut Caps, bounded: &mut Vec<(Var, Caps)>) {
        let mut go = |t: &Type, out: &mut Caps| self.reach_into(t, out, bounded);
        match t {
            Type::Var(v) => {
                out.atoms.insert(v.clone());
            }
            Type::App(..) => {
                let mut args = Vec::new();
                let mut head = t;
                while let Type::App(h, a) = head {
                    args.push(&**a);
                    head = h;
                }
                let params = match head {
                    Type::Con(p) => self.decls.params.get(p),
                    _ => None,
                };
                go(head, out);
                // an argument of kind SRow reaches nothing (§2.1)
                for (i, a) in args.iter().rev().enumerate() {
                    if params.and_then(|k| k.get(i)) != Some(&Kind::Services) {
                        go(a, out);
                    }
                }
            }
            Type::Fun(a, s, b) => {
                go(a, out);
                go(b, out);
                if let Stage::Runtime(r, _) = s {
                    *out = out.union(&row_live(r));
                }
            }
            Type::Eff(r, _, a) => {
                go(a, out);
                *out = out.union(&row_live(r));
            }
            Type::Ref(a) => go(a, out),
            Type::Record(f) => f.fields.values().for_each(|t| go(t, out)),
            Type::Tuple(ts) => ts.iter().for_each(|t| go(t, out)),
            // the variables a `∀` binds are no captures: each use instantiates them
            Type::Forall(s) => {
                let mut inner = Caps::default();
                let mut under = Vec::new();
                self.reach_into(&s.body, &mut inner, &mut under);
                let bound =
                    |v: &Var| matches!(v, Var::Named(n) if s.vars.iter().any(|(b, _)| b == n));
                for (a, c) in under {
                    if bound(&a) {
                        inner = inner.union(&c);
                    } else {
                        bounded.push((a, c));
                    }
                }
                inner.atoms.retain(|v| !bound(v));
                inner.vars.retain(|v| !bound(v));
                *out = out.union(&inner);
            }
            Type::Captured(t, c) => {
                go(t, out);
                if self.hidden(t, &mut Vec::new()) {
                    *out = out.union(c);
                }
            }
            Type::Bounded(t, c) => match self.resolve(t) {
                Type::Var(v @ Var::Named(_)) if !self.is_region(&v) => bounded.push((v, c.clone())),
                _ if self.holds_nothing(t) => {}
                _ => {
                    let mut inner = Caps::default();
                    go(t, &mut inner);
                    inner.atoms.retain(|v| self.is_region(v));
                    inner.vars.clear();
                    *out = out.union(&inner).union(c);
                }
            },
            // `Errors {E ῡ} ≡ E ῡ` (`core.md` §1.5) reaches what its errors
            // do, and a variable of the row what the errors it stands for do
            // (`docs/formal/regions.md#errors-reaches-its-payloads`)
            Type::Errors(r) => {
                for args in r.elems.values() {
                    args.iter().for_each(|a| go(a, out));
                }
                out.atoms.extend(r.vars.iter().cloned());
            }
            // a row is no capture; `Layer`, implementations, tags and
            // dictionaries hold nothing (§2.1)
            Type::Con(_)
            | Type::EffOf(..)
            | Type::Layer(..)
            | Type::Impl(_)
            | Type::Tag(_)
            | Type::Dict(..)
            | Type::Env(_) => {}
        }
    }

    pub(super) fn is_region(&self, v: &Var) -> bool {
        self.regions.kinds.contains(v)
    }

    fn scalar(&self, t: &Type) -> bool {
        matches!(self.resolve(t), Type::Con(p) if p.module.as_ref() == "Prelude" && SCALARS.contains(&p.name.as_ref()))
    }

    /// A qualifier on a value that can hold nothing adds nothing (`docs/formal/regions.md#a-value-bound-survives-an-unknown-type-position`).
    fn holds_nothing(&self, t: &Type) -> bool {
        self.scalar(t)
            || (!matches!(self.resolve(t), Type::Var(_))
                && self.reach(t).is_empty()
                && !self.hidden(t, &mut Vec::new()))
    }

    /// Whether a value of `t` stores what its type does not show: a
    /// function, a computation, a cell, a type without constructors, or
    /// data with such a field (§1.2).
    pub(super) fn hidden(&self, t: &Type, seen: &mut Vec<Path>) -> bool {
        match self.resolve(t) {
            Type::Var(_) => false,
            Type::Tuple(ts) => ts.iter().any(|t| self.hidden(t, seen)),
            Type::Record(f) => f.fields.values().any(|t| self.hidden(t, seen)),
            Type::Forall(s) => self.hidden(&s.body, seen),
            t @ (Type::Con(_) | Type::App(..)) => {
                let mut args = Vec::new();
                let mut head = t;
                while let Type::App(h, a) = head {
                    args.push((*a).clone());
                    head = self.resolve(&h);
                }
                args.reverse();
                let Type::Con(p) = head else { return true };
                if p.module.as_ref() == "Prelude" && SCALARS.contains(&p.name.as_ref()) {
                    return false;
                }
                if p == Path::new("Prelude", "List") {
                    return args.iter().any(|t| self.hidden(t, seen));
                }
                if seen.contains(&p) {
                    return false;
                }
                let Some(fields) = self.fields_of(&p, &args) else {
                    return true;
                };
                seen.push(p);
                fields.iter().any(|t| self.hidden(t, seen))
            }
            _ => true,
        }
    }

    /// The fields of every constructor of the declared type `p` at `args`;
    /// `None` for a type without constructors.
    fn fields_of(&self, p: &Path, args: &[Type]) -> Option<Vec<Type>> {
        let variants = self.decls.variants.get(p)?;
        if self.decls.errors.contains(p) {
            return None;
        }
        let mut out = Vec::new();
        for (name, _) in variants {
            let s = self.decls.constructors.get(&Path {
                module: p.module.clone(),
                name: name.clone(),
            })?;
            let mut params = Vec::new();
            let mut t = &s.body;
            while let Type::Fun(x, _, r) = t {
                params.push((**x).clone());
                t = r;
            }
            let mut result = Vec::new();
            while let Type::App(h, a) = t {
                result.push((**a).clone());
                t = h;
            }
            result.reverse();
            let map = result
                .iter()
                .zip(args)
                .filter_map(|(v, a)| match v {
                    Type::Var(Var::Named(n)) => Some((n.clone(), super::Inst::Type(a.clone()))),
                    _ => None,
                })
                .collect();
            out.extend(params.iter().map(|x| super::substitute(x, &map)));
        }
        Some(out)
    }

    /// Records `⋃ ⌈held⌉ ⊆ ⋃ ⌈within⌉ ∪ allowed`, decided at the end.
    pub(super) fn hold(
        &mut self,
        held: &[Type],
        within: &[Type],
        allowed: &Caps,
        rule: &'static str,
    ) {
        self.regions.holds.push(Hold {
            held: held.to_vec(),
            within: within.to_vec(),
            allowed: allowed.clone(),
            rule,
        });
    }

    /// A capture row inference has yet to solve.
    pub(super) fn fresh_caps(&mut self) -> Caps {
        Caps::var(Var::Meta(self.meta()))
    }

    /// The qualifier of `t`, `∅` when bare.
    pub(super) fn qualifier(&self, t: &Type) -> Caps {
        match self.follow(t) {
            Type::Captured(_, c) | Type::Bounded(_, c) => self.zonk_caps(&c),
            _ => Caps::default(),
        }
    }

    /// Every qualifier of `t`, outermost first: a value of `(τ^{≤C})^{≤D}`,
    /// as `Ref.set` takes into a cell of `τ^{≤C}`, is within each.
    pub(super) fn qualifiers(&self, t: &Type) -> Vec<Caps> {
        let mut out = Vec::new();
        let mut t = self.follow(t);
        while let Type::Captured(inner, c) | Type::Bounded(inner, c) = &t {
            out.push(self.zonk_caps(c));
            t = self.follow(inner);
        }
        out
    }

    /// `t` once for each of its qualifiers, about the bare type: what is
    /// within `(τ^{≤C})^{≤D}` is within `τ^{≤C}` and within `τ^{≤D}`, not
    /// within what the two reach together.
    pub(super) fn layers(&self, t: &Type) -> Vec<Type> {
        let mut out = Vec::new();
        let bare = Rc::new(self.resolve(t));
        let mut cur = self.follow(t);
        loop {
            cur = match &cur {
                Type::Captured(inner, c) => {
                    out.push(Type::Captured(bare.clone(), c.clone()));
                    self.follow(inner)
                }
                Type::Bounded(inner, c) => {
                    out.push(Type::Bounded(bare.clone(), c.clone()));
                    self.follow(inner)
                }
                _ => break,
            };
        }
        if out.len() < 2 {
            return vec![self.follow(t)];
        }
        out
    }

    /// The premise of (Lam) and (Task) against `expected`, for each of its
    /// qualifiers: the body's stage `inner` within `stage` with its `Live`.
    /// What the body holds is no bound: a qualifier is what its use needs.
    pub(super) fn close_within(
        &mut self,
        ty: &Type,
        inner: &Stage,
        stage: &Stage,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let mut each = expected.map_or_else(Vec::new, |t| self.qualifiers(t));
        if each.is_empty() {
            each.push(Caps::default());
        }
        for q in &each {
            self.stage_within(inner, &live_stage(stage, q))?;
        }
        Ok(ty.clone())
    }

    /// (Proj): a part of a value of `(T ῡ)^C` taken apart has `C` too.
    pub(super) fn project_part(&self, whole: &Type, part: Type) -> Type {
        match self.follow(whole) {
            Type::Captured(t, c) | Type::Bounded(t, c)
                if !c.is_empty() && self.hidden(&t, &mut Vec::new()) =>
            {
                Type::Captured(Rc::new(part), c)
            }
            _ => part,
        }
    }

    /// (Con): the fields of `K ē`; captures a field hides go into the
    /// qualifier of the data, which a type without hidden storage needs not.
    pub(super) fn construct(&mut self, c: &Con, args: &[Term], at: &Term) -> Check<Type> {
        let mut t = self.constructor(&c.ty.module, &c.name)?;
        self.note_instance(at);
        let mut fields = Vec::new();
        for a in args {
            let Type::Fun(p, _, r) = self.resolve(&t) else {
                return error("Con", format!("`{}` takes fewer fields", c.name));
            };
            let field = if self.hidden(&p, &mut Vec::new()) {
                let q = self.fresh_caps();
                Type::Captured(p, q)
            } else {
                (*p).clone()
            };
            self.check(a, &field)?;
            fields.push(field);
            t = (*r).clone();
        }
        if !self.hidden(&t, &mut Vec::new()) {
            return Ok(t);
        }
        let q = self.fresh_caps();
        self.hold(&fields, &[t.clone()], &q, "Con");
        Ok(Type::Captured(Rc::new(t), q))
    }

    /// (Inst): each `αᵢ^D` of the scheme bounds what `αᵢ` becomes, and `Θ`
    /// is needed at the instance.
    pub(super) fn instance_premises(&mut self, s: &Scheme, map: &HashMap<Name, super::Inst>) {
        let mut bounds = Vec::new();
        bounded_vars(&s.body, &s.vars, &mut bounds);
        for (a, d) in bounds {
            let t = super::substitute(&Type::Var(Var::Named(a)), map);
            let d = super::generalize::substitute_caps(&d, map);
            self.hold(&[t], &[], &d, "Inst");
        }
        for (c, r) in &s.outlives {
            let c = super::generalize::substitute_caps(c, map);
            let r = super::substitute(r, map);
            let facts = self.regions.facts.clone();
            self.regions.needs.push((c, r, facts));
        }
    }
}

/// What `{Live C}` of `r` needs alive: `C`, or nothing.
fn row_live(r: &Row) -> Caps {
    r.elems
        .get(&live_service())
        .map(|a| arg_caps(&a[0]))
        .unwrap_or_default()
}

/// The variables `α` of a scheme under a value bound `α^D`.
fn bounded_vars(t: &Type, vars: &[(Name, Kind)], out: &mut Vec<(Name, Caps)>) {
    let go = |t: &Type, out: &mut Vec<(Name, Caps)>| bounded_vars(t, vars, out);
    match t {
        Type::Bounded(x, d) => {
            if let Type::Var(Var::Named(a)) = &**x
                && vars.iter().any(|(v, _)| v == a)
            {
                out.push((a.clone(), d.clone()));
            }
            go(x, out);
        }
        Type::Captured(x, _) | Type::Eff(_, _, x) | Type::Ref(x) => go(x, out),
        Type::App(a, b) | Type::Fun(a, _, b) => {
            go(a, out);
            go(b, out);
        }
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().for_each(|t| go(t, out)),
        Type::Record(Fields { fields, .. }) => fields.values().for_each(|t| go(t, out)),
        _ => {}
    }
}
