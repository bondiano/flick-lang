//! Instances of schemes (`docs/formal/core.md` §2.3 (Var), (Let-Sig)):
//! `τ[ᾱ := τ̄]` with fresh metas or with the rigid variables of a
//! signature, and `Gen` (§2.5) by levels: a meta made inside the right
//! side of a `let` and not reachable from `Γ` is quantified.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use super::{Checker, Solved};
use crate::syntax::Name;
use crate::types::{Fields, Kind, Row, Scheme, Stage, Type, Var};

/// What a variable of a scheme becomes.
#[derive(Clone)]
pub(crate) enum Inst {
    Type(Type),
    Row(Row),
    Tail(Var),
}

/// What a variable of kind `k` becomes as the rigid variable `name`.
pub(super) fn rigid_inst(name: &str, k: &Kind) -> Inst {
    let v = Var::Named(name.into());
    match k {
        Kind::Services | Kind::Errors => Inst::Row(Row::var(v)),
        Kind::Fields => Inst::Tail(v),
        _ => Inst::Type(Type::Var(v)),
    }
}

impl Checker<'_> {
    fn fresh_inst(&mut self, k: &Kind) -> Inst {
        let v = Var::Meta(self.meta());
        match k {
            Kind::Services | Kind::Errors => Inst::Row(Row::var(v)),
            Kind::Fields => Inst::Tail(v),
            _ => Inst::Type(Type::Var(v)),
        }
    }

    /// `τ[ᾱ := τ̄]` with fresh metas: the instance (Var) takes.
    pub(super) fn instantiate(&mut self, s: &Scheme) -> Type {
        let insts: Vec<Inst> = s.vars.iter().map(|(_, k)| self.fresh_inst(k)).collect();
        substitute_all(&s.body, &s.vars, &insts)
    }

    /// The body of `s` with its variables rigid: what (Let-Sig) checks.
    pub(super) fn skolemize(&mut self, s: &Scheme) -> Type {
        let insts: Vec<Inst> = s
            .vars
            .iter()
            .map(|(n, k)| {
                self.rigid += 1;
                rigid_inst(&format!("{n}'{}", self.rigid), k)
            })
            .collect();
        substitute_all(&s.body, &s.vars, &insts)
    }

    /// `Gen(Γ, τ)`: the type metas of `t` deeper than the current level.
    /// Rows are not quantified: they are settled to the least (Р11).
    pub(super) fn generalize(&mut self, t: &Type) -> Scheme {
        let mut ms = BTreeSet::new();
        self.metas(t, &mut ms);
        let mut vars = Vec::new();
        for m in ms {
            let slot = &self.slots[m as usize];
            if slot.level > self.level && slot.solved.is_none() && self.is_type_meta(m, t) {
                let name: Name = format!("g{m}").into();
                self.slots[m as usize].solved =
                    Some(Solved::Type(Type::Var(Var::Named(name.clone()))));
                vars.push((name, Kind::Type));
            }
        }
        Scheme {
            vars,
            body: self.zonk(t),
        }
    }

    /// Whether meta `m` stands for a type in `t`, not for a row.
    fn is_type_meta(&self, m: u32, t: &Type) -> bool {
        fn occurs(c: &Checker, m: u32, t: &Type) -> bool {
            match c.zonk(t) {
                Type::Var(Var::Meta(n)) => n == m,
                Type::App(h, a) => occurs(c, m, &h) || occurs(c, m, &a),
                Type::Fun(p, _, r) => occurs(c, m, &p) || occurs(c, m, &r),
                Type::Eff(_, _, t) | Type::Ref(t) => occurs(c, m, &t),
                Type::Record(f) => f.fields.values().any(|t| occurs(c, m, t)),
                Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().any(|t| occurs(c, m, t)),
                Type::Forall(s) => occurs(c, m, &s.body),
                _ => false,
            }
        }
        occurs(self, m, t)
    }
}

pub(super) fn substitute_all(t: &Type, vars: &[(Name, Kind)], insts: &[Inst]) -> Type {
    let map: HashMap<Name, Inst> = vars
        .iter()
        .map(|(n, _)| n.clone())
        .zip(insts.iter().cloned())
        .collect();
    substitute(t, &map)
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
            _ => t.clone(),
        },
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
