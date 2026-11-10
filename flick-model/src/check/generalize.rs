//! Instances of schemes (`docs/formal/core.md` §2.3 (Var), (Let-Sig)):
//! `τ[ᾱ := τ̄]` with fresh metas or with the rigid variables of a
//! signature, and `Gen` (§2.5) by levels: a meta made inside the right
//! side of a `let` and not reachable from `Γ` is quantified.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use super::regions::regions_of;
use super::{Checker, Solved};
use crate::syntax::Name;
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

/// What a variable of a scheme becomes.
#[derive(Clone, Debug)]
pub(crate) enum Inst {
    Type(Type),
    Row(Row),
    Tail(Var),
    Caps(Caps),
}

/// What a variable of kind `k` becomes as the rigid variable `name`.
pub(super) fn rigid_inst(name: &str, k: &Kind) -> Inst {
    let v = Var::Named(name.into());
    match k {
        Kind::Services | Kind::Errors => Inst::Row(Row::var(v)),
        Kind::Fields => Inst::Tail(v),
        Kind::Captures => Inst::Caps(Caps::var(v)),
        _ => Inst::Type(Type::Var(v)),
    }
}

impl Checker<'_> {
    pub(super) fn fresh_inst(&mut self, k: &Kind) -> Inst {
        let v = Var::Meta(self.meta());
        if *k == Kind::Region {
            self.regions.kinds.insert(v.clone());
        }
        match k {
            Kind::Services | Kind::Errors => Inst::Row(Row::var(v)),
            Kind::Fields => Inst::Tail(v),
            Kind::Captures => Inst::Caps(Caps::var(v)),
            _ => Inst::Type(Type::Var(v)),
        }
    }

    /// `τ[ᾱ := τ̄]` with fresh metas: the instance (Var) takes, with the
    /// premises of (Inst) on it (`regions.md` §2.3).
    pub(super) fn instantiate(&mut self, s: &Scheme) -> Type {
        let insts: Vec<Inst> = s.vars.iter().map(|(_, k)| self.fresh_inst(k)).collect();
        let map = names(&s.vars, &insts);
        self.instance_premises(s, &map);
        substitute(&s.body, &map)
    }

    /// [`Self::instantiate`], and what each variable became.
    pub(super) fn instantiate_with(&mut self, s: &Scheme) -> (Type, Vec<Inst>) {
        let insts: Vec<Inst> = s.vars.iter().map(|(_, k)| self.fresh_inst(k)).collect();
        let map = names(&s.vars, &insts);
        self.instance_premises(s, &map);
        (substitute(&s.body, &map), insts)
    }

    /// The body of `s` with its variables rigid and the stages of its
    /// signature holding their parameters: what (Let-Sig) checks, with the
    /// names of the rigid variables. Its facts `Θ` are given from here on.
    pub(super) fn skolemize(&mut self, s: &Scheme) -> (Type, Vec<Name>) {
        let mut rigid = Vec::new();
        let insts: Vec<Inst> = s
            .vars
            .iter()
            .map(|(n, k)| {
                self.rigid += 1;
                let name = format!("{n}'{}", self.rigid);
                if *k == Kind::Region {
                    self.regions.kinds.insert(Var::Named(name.as_str().into()));
                }
                rigid.push(Name::from(name.as_str()));
                rigid_inst(&name, k)
            })
            .collect();
        let map = names(&s.vars, &insts);
        for (c, r) in &s.outlives {
            let fact = (substitute_caps(c, &map), substitute(r, &map));
            self.regions.facts.push(fact);
        }
        let body = self.elide(
            &s.body,
            &Caps::default(),
            &regions_of(&s.vars, &BTreeSet::new()),
        );
        let body = substitute(&body, &map);
        self.give_provided(&body);
        (body, rigid)
    }

    /// `Gen(Γ, τ)`: the type metas of `t` deeper than the current level.
    /// Rows are not quantified: they are settled to the least (`docs/formal/core.md#unknown-call-rows-in-a-generalized-binding-close-to-none`).
    pub(super) fn generalize(&mut self, t: &Type) -> Scheme {
        let mut ms = BTreeSet::new();
        self.metas(t, &mut ms);
        let zonked = self.zonk(t);
        let mut vars = Vec::new();
        for m in ms {
            let slot = &self.slots[m as usize];
            if slot.level <= self.level || slot.solved.is_some() {
                continue;
            }
            let name: Name = format!("g{m}").into();
            let named = Var::Named(name.clone());
            if self.is_type_meta(m, t) {
                self.slots[m as usize].solved = Some(Solved::Type(Type::Var(named)));
                vars.push((name, Kind::Type));
            } else if in_errors(&Var::Meta(m), &zonked) {
                // a row a type takes, as `Errors e`; a row only of a stage
                // is closed instead (`core.md` §2.5, item 5)
                self.slots[m as usize].solved = Some(Solved::Row(Row::var(named)));
                vars.push((name, Kind::Errors));
            }
        }
        Scheme {
            vars,
            body: self.zonk(t),
            outlives: Vec::new(),
        }
    }

    /// Whether meta `m` stands for a type in `t`, not for a row.
    fn is_type_meta(&self, m: u32, t: &Type) -> bool {
        self.mentions(&Var::Meta(m), t)
    }

    /// Whether the variable `v` stands for a type in `t`, not for a row.
    pub(super) fn mentions(&self, v: &Var, t: &Type) -> bool {
        match self.zonk(t) {
            Type::Var(x) => x == *v,
            Type::App(h, a) => self.mentions(v, &h) || self.mentions(v, &a),
            Type::Fun(p, _, r) => self.mentions(v, &p) || self.mentions(v, &r),
            Type::Eff(_, _, t) | Type::Ref(t) | Type::Captured(t, _) | Type::Bounded(t, _) => {
                self.mentions(v, &t)
            }
            Type::Record(f) => f.fields.values().any(|t| self.mentions(v, t)),
            Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().any(|t| self.mentions(v, t)),
            Type::Forall(s) => self.mentions(v, &s.body),
            _ => false,
        }
    }
}

/// Whether the row variable `v` is in a row a type of `t` takes, `Errors ρ`.
fn in_errors(v: &Var, t: &Type) -> bool {
    let any = |ts: &[Type]| ts.iter().any(|t| in_errors(v, t));
    match t {
        Type::Errors(r) => r.vars.contains(v) || r.elems.values().any(|a| any(a)),
        Type::App(h, a) | Type::Fun(h, _, a) => in_errors(v, h) || in_errors(v, a),
        Type::Eff(_, _, t) | Type::Ref(t) | Type::Captured(t, _) | Type::Bounded(t, _) => {
            in_errors(v, t)
        }
        Type::Record(f) => f.fields.values().any(|t| in_errors(v, t)),
        Type::Tuple(ts) | Type::Dict(_, ts) => any(ts),
        Type::Forall(s) => in_errors(v, &s.body),
        _ => false,
    }
}

pub(super) fn substitute_all(t: &Type, vars: &[(Name, Kind)], insts: &[Inst]) -> Type {
    substitute(t, &names(vars, insts))
}

fn names(vars: &[(Name, Kind)], insts: &[Inst]) -> HashMap<Name, Inst> {
    vars.iter()
        .map(|(n, _)| n.clone())
        .zip(insts.iter().cloned())
        .collect()
}

/// `C[ᾱ := τ̄, c̄ := C̄, ē := ρ̄]`: an atom or tail that becomes a type or
/// a row of errors is what that type or row names.
pub(super) fn substitute_caps(c: &Caps, map: &HashMap<Name, Inst>) -> Caps {
    let mut out = Caps::default();
    let all = c.atoms.iter().map(|v| (v, true));
    for (v, atom) in all.chain(c.vars.iter().map(|v| (v, false))) {
        let inst = match v {
            Var::Named(n) => map.get(n),
            Var::Meta(_) => None,
        };
        match inst {
            Some(Inst::Caps(more)) => out = out.union(more),
            Some(Inst::Type(t)) => free_atoms(t, &mut out),
            Some(Inst::Row(r)) => row_atoms(r, &mut out),
            _ if atom => {
                out.atoms.insert(v.clone());
            }
            _ => {
                out.vars.insert(v.clone());
            }
        }
    }
    out
}

/// The variables of `t` as atoms, its qualifiers as they are: `⌈t⌉` of a
/// type without hidden storage.
fn free_atoms(t: &Type, out: &mut Caps) {
    match t {
        Type::Var(v) => {
            out.atoms.insert(v.clone());
        }
        Type::App(a, b) | Type::Fun(a, _, b) => {
            free_atoms(a, out);
            free_atoms(b, out);
        }
        Type::Eff(_, _, x) | Type::Ref(x) => free_atoms(x, out),
        Type::Captured(x, c) | Type::Bounded(x, c) => {
            *out = out.union(c);
            free_atoms(x, out);
        }
        Type::Tuple(ts) => ts.iter().for_each(|t| free_atoms(t, out)),
        Type::Record(f) => f.fields.values().for_each(|t| free_atoms(t, out)),
        Type::Errors(r) => row_atoms(r, out),
        _ => {}
    }
}

/// `⌈Errors ρ⌉`: what the errors of `ρ` name and its variables.
fn row_atoms(r: &Row, out: &mut Caps) {
    for args in r.elems.values() {
        args.iter().for_each(|t| free_atoms(t, out));
    }
    out.atoms.extend(r.vars.iter().cloned());
}

/// `τ[ᾱ := τ̄]`.
pub(crate) fn substitute(t: &Type, map: &HashMap<Name, Inst>) -> Type {
    if map.is_empty() {
        return t.clone();
    }
    let s = |t: &Rc<Type>| Rc::new(substitute(t, map));
    match t {
        Type::Var(Var::Named(n)) => match map.get(n) {
            Some(Inst::Type(t)) => t.clone(),
            // a row as the argument of a type: `Fiber e a`, `Failure e`
            Some(Inst::Row(r)) => Type::Errors(r.clone()),
            _ => t.clone(),
        },
        Type::Captured(x, c) => Type::Captured(s(x), substitute_caps(c, map)),
        Type::Bounded(x, c) => Type::Bounded(s(x), substitute_caps(c, map)),
        Type::Var(_) | Type::Con(_) => t.clone(),
        Type::App(h, a) => Type::App(s(h), s(a)),
        Type::Fun(p, st, r) => Type::Fun(s(p), substitute_stage(st, map), s(r)),
        Type::Eff(r, e, x) => Type::Eff(substitute_row(r, map), substitute_row(e, map), s(x)),
        Type::EffOf(r, e) => Type::EffOf(substitute_row(r, map), substitute_row(e, map)),
        Type::Ref(x) => Type::Ref(s(x)),
        Type::Layer(i, e, o) => Type::Layer(
            substitute_row(i, map),
            substitute_row(e, map),
            substitute_row(o, map),
        ),
        Type::Record(f) => Type::Record(Fields {
            fields: f
                .fields
                .iter()
                .map(|(l, t)| (l.clone(), substitute(t, map)))
                .collect(),
            tail: match &f.tail {
                Some(Var::Named(n)) => match map.get(n) {
                    Some(Inst::Tail(v)) => Some(v.clone()),
                    _ => f.tail.clone(),
                },
                other => other.clone(),
            },
        }),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| substitute(t, map)).collect()),
        Type::Errors(r) => Type::Errors(substitute_row(r, map)),
        Type::Impl(r) => Type::Impl(substitute_row(r, map)),
        Type::Tag(r) => Type::Tag(substitute_row(r, map)),
        Type::Dict(c, ts) => Type::Dict(c.clone(), ts.iter().map(|t| substitute(t, map)).collect()),
        Type::Env(r) => Type::Env(substitute_row(r, map)),
        Type::Forall(sc) => {
            let inner: HashMap<Name, Inst> = map
                .iter()
                .filter(|(n, _)| !sc.vars.iter().any(|(v, _)| v == *n))
                .map(|(n, i)| (n.clone(), i.clone()))
                .collect();
            Type::Forall(Rc::new(Scheme {
                vars: sc.vars.clone(),
                body: substitute(&sc.body, &inner),
                outlives: sc
                    .outlives
                    .iter()
                    .map(|(c, r)| (substitute_caps(c, &inner), substitute(r, &inner)))
                    .collect(),
            }))
        }
    }
}

fn substitute_stage(s: &Stage, map: &HashMap<Name, Inst>) -> Stage {
    match s {
        Stage::Pure => Stage::Pure,
        Stage::Runtime(r, e) => Stage::Runtime(substitute_row(r, map), substitute_row(e, map)),
    }
}

pub(super) fn substitute_row(r: &Row, map: &HashMap<Name, Inst>) -> Row {
    let mut out = Row {
        elems: r
            .elems
            .iter()
            .map(|(n, args)| (n.clone(), args.iter().map(|a| substitute(a, map)).collect()))
            .collect(),
        vars: BTreeSet::new(),
    };
    for v in &r.vars {
        match v {
            Var::Named(n) => match map.get(n) {
                Some(Inst::Row(inner)) => {
                    for (k, args) in &inner.elems {
                        out.elems.entry(k.clone()).or_insert_with(|| args.clone());
                    }
                    out.vars.extend(inner.vars.iter().cloned());
                }
                _ => {
                    out.vars.insert(v.clone());
                }
            },
            Var::Meta(_) => {
                out.vars.insert(v.clone());
            }
        }
    }
    out
}

/// The parameters of a class as `args`.
pub(super) fn params(params: &[Name], args: &[Type]) -> HashMap<Name, Inst> {
    params
        .iter()
        .cloned()
        .zip(args.iter().map(|a| Inst::Type(a.clone())))
        .collect()
}
