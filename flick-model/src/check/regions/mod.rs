//! Regions and captures (`docs/formal/regions.md` §2), written from the
//! document and not from flick-core's `types/regions/`: ⌈·⌉ (§2.1), the
//! obligations of (≤-Cap), (Lam), (Task), (Con) and (Inst) and their least
//! solution (§2.6), (Scope) (§2.3), `Outlives` (§2.4) and (Err-Raise)
//! (§2.5). An obligation waits for the end of the declaration, when every
//! type it names is known: a capture row grows to the least set its
//! obligations need, and a meta outside a `scope` never takes its region.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::{Check, Checker, error};

mod data;
mod live;
mod provided;
mod qualifiers;
mod scope;
mod solve;
use crate::syntax::{Con, Name, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};
pub(crate) use live::live_service;
pub(in crate::check) use live::{
    add_live, arg_caps, drop_empty_live, live_arg, live_stage, regions_of,
};
pub(in crate::check) use scope::names_into;

/// `⋃ ⌈held⌉ ⊆ ⋃ ⌈within⌉ ∪ allowed`.
#[derive(Clone, Debug)]
pub(super) struct Hold {
    held: Vec<Type>,
    within: Vec<Type>,
    allowed: Caps,
    rule: &'static str,
    givens: super::row_facts::Givens,
    /// Scheme-bound facts in force where the obligation was generated.
    bounds: Vec<(Var, Caps)>,
}

/// `Outlives C ϱ` and the facts in force where it is needed.
type Need = (Caps, Type, Vec<(Caps, Type)>);

/// A row obligation whose owner is not known until its enclosing call closes.
#[derive(Clone)]
struct ProvidedNeed {
    row: Row,
    within: Row,
    givens: Vec<(Row, Type)>,
}

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
    /// Value bounds of rigid variables introduced by a signed declaration.
    bounds: Vec<(Var, Caps)>,
    pending_provided: Vec<ProvidedNeed>,
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
        self.reach_into(&self.zonk(t), &mut out, &mut bounded, false);
        (out, bounded)
    }

    /// What a position admits: a bound on a complete value permits its C;
    /// a bounded variable admits only that same variable (regions.md §2.1).
    pub(super) fn reach_position(&self, t: &Type) -> (Caps, Vec<(Var, Caps)>) {
        let mut out = Caps::default();
        let mut bounded = Vec::new();
        self.reach_into(&self.zonk(t), &mut out, &mut bounded, true);
        (out, bounded)
    }

    fn reach_into(&self, t: &Type, out: &mut Caps, bounded: &mut Vec<(Var, Caps)>, position: bool) {
        let mut go = |t: &Type, out: &mut Caps| self.reach_into(t, out, bounded, position);
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
                self.reach_into(&s.body, &mut inner, &mut under, position);
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
                if self.hidden(t, &mut Vec::new()) {
                    go(t, out);
                    *out = out.union(c);
                } else {
                    self.reach_into(&Type::Bounded(t.clone(), c.clone()), out, bounded, position);
                }
            }
            Type::Bounded(t, c) => match self.resolve(t) {
                Type::Var(v @ Var::Named(_)) if !self.is_region(&v) => bounded.push((v, c.clone())),
                _ if self.holds_nothing(t) => {}
                _ => {
                    let mut inner = Caps::default();
                    go(t, &mut inner);
                    if position {
                        // The position admits what its complete type shows
                        // (a of List a), as well as its written bound.
                        *out = out.union(&inner).union(c);
                        return;
                    }
                    // Refining a complete field type never creates hidden
                    // storage in it. Keep its declared storage and regions;
                    // the bound constrains its remaining abstract atoms.
                    for atom in &inner.atoms {
                        if !self.is_region(atom) {
                            bounded.push((atom.clone(), c.clone()));
                        }
                    }
                    inner.atoms.retain(|v| self.is_region(v));
                    *out = out.union(&inner);
                }
            },
            // `Errors {E ῡ} ≡ E ῡ` (`core.md` §1.5) reaches what its errors
            // do, and a variable of the row what the errors it stands for do
            // (`docs/formal/regions.md#errors-reaches-its-payloads`)
            Type::Caps(c) => *out = out.union(c),
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
    pub(super) fn holds_nothing(&self, t: &Type) -> bool {
        self.scalar(t)
            || (!matches!(self.resolve(t), Type::Var(_))
                && self.reach(t).is_empty()
                && !self.hidden(t, &mut Vec::new()))
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
            givens: self.branch_givens(),
            bounds: self.regions.bounds.clone(),
        });
    }

    /// A capture row inference has yet to solve.
    pub(super) fn fresh_caps(&mut self) -> Caps {
        Caps::var(Var::Meta(self.meta()))
    }

    /// (Con): the fields of `K ē`; captures a field hides go into the
    /// qualifier of the data, which a type without hidden storage needs not.
    pub(super) fn construct<'e>(
        &mut self,
        c: &Con,
        args: impl IntoIterator<Item = &'e Term>,
        at: &Term,
    ) -> Check<Type> {
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
        self.remember_row_instances(&super::substitute(&s.body, map));
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
    /// A signed body assumes its scheme's bound facts, just as it assumes
    /// Outlives and Provided. Uses of the scheme still prove (Inst).
    pub(super) fn give_bounds(&mut self, s: &Scheme, map: &HashMap<Name, super::Inst>) {
        let mut bounds = Vec::new();
        bounded_vars(&s.body, &s.vars, &mut bounds);
        for (name, bound) in bounds {
            let t = super::substitute(&Type::Var(Var::Named(name)), map);
            if let Type::Var(variable) = t {
                let bound = super::generalize::substitute_caps(&bound, map);
                self.regions.bounds.push((variable, bound));
            }
        }
    }

    /// Variables of pending region obligations remain monomorphic (Gen,
    /// core.md §2.5 item 4). A later argument/call can still fix their owner.
    pub(super) fn open_region_metas(&self) -> BTreeSet<u32> {
        let mut metas = BTreeSet::new();
        for hold in &self.regions.holds {
            for held in &hold.held {
                self.metas(held, &mut metas);
            }
        }
        for (captures, owner, _) in &self.regions.needs {
            if !self.zonk_caps(captures).is_empty() {
                self.metas(owner, &mut metas);
                self.metas(
                    &Type::Captured(Rc::new(Type::con("Prelude", "Unit")), captures.clone()),
                    &mut metas,
                );
            }
        }
        for need in &self.regions.pending_provided {
            self.metas(&Type::Errors(need.row.clone()), &mut metas);
            self.metas(&Type::Errors(need.within.clone()), &mut metas);
        }
        metas
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
