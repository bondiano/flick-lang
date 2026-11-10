//! Following metas and replacing solved ones (`docs/formal/core.md`
//! §1.2); an opaque alias of the module being checked is its
//! representation there (`docs/formal/elaboration.md#an-opaque-alias-inside-its-module-resolves-through-its-representation`).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::dicts::arg;
use super::generalize::substitute;
use super::regions::drop_empty_live;
use super::{Checker, Solved};
use crate::syntax::{Name, Path};
use crate::types::{Caps, Fields, Row, Scheme, Stage, Type, Var};

impl Checker<'_> {
    pub(super) fn solution(&self, v: &Var) -> Option<&Solved> {
        match v {
            Var::Meta(m) => self.slots[*m as usize].solved.as_ref(),
            Var::Named(n) => self
                .equalities
                .iter()
                .rev()
                .find(|(x, _)| x == n)
                .map(|(_, s)| s),
        }
    }

    /// `t` with the metas at its head followed, and `EffOf ρ ρ' τ` as
    /// `Eff ρ ρ' τ`; a qualifier is kept, and an opaque alias of the module
    /// is its representation.
    pub(super) fn follow(&self, t: &Type) -> Type {
        let t = self.heads(t);
        match self.unfold(&t) {
            Some(u) => self.follow(&u),
            None => t,
        }
    }

    /// [`Checker::follow`] but for opaque aliases, which stay nominal: what
    /// walks a whole type, as an alias may name itself.
    pub(super) fn heads(&self, t: &Type) -> Type {
        let mut t = t.clone();
        while let Type::Var(v) = &t
            && let Some(Solved::Type(next)) = self.solution(v)
        {
            t = next.clone();
        }
        if let Type::App(h, a) = &t
            && let Type::EffOf(r, e) = self.resolve(h)
        {
            return Type::Eff(r, e, a.clone());
        }
        t
    }

    /// `t` followed, under its qualifiers: what a rule that takes the
    /// value apart sees.
    pub(super) fn resolve(&self, t: &Type) -> Type {
        let mut t = self.follow(t);
        while let Type::Captured(inner, _) | Type::Bounded(inner, _) = &t {
            t = self.follow(inner);
        }
        t
    }

    /// `C` with every solved meta replaced: a type meta in an atom by
    /// what its type reaches (`regions.md` §2.1, the reach bound).
    pub(super) fn zonk_caps(&self, c: &Caps) -> Caps {
        let mut out = Caps::default();
        for a in &c.atoms {
            match self.solution(a) {
                Some(Solved::Type(t)) => out = out.union(&self.reach(t)),
                Some(Solved::Row(r)) => out = out.union(&self.reach(&Type::Errors(r.clone()))),
                _ => {
                    out.atoms.insert(a.clone());
                }
            }
        }
        for v in &c.vars {
            match self.solution(v) {
                Some(Solved::Caps(more)) => out = out.union(&self.zonk_caps(more)),
                Some(Solved::Type(t)) => out = out.union(&self.reach(t)),
                _ => {
                    out.vars.insert(v.clone());
                }
            }
        }
        out
    }

    /// `t` with every solved meta replaced.
    pub(super) fn zonk(&self, t: &Type) -> Type {
        let z = |t: &Rc<Type>| Rc::new(self.zonk(t));
        match self.heads(t) {
            Type::Captured(t, c) => Type::Captured(z(&t), self.zonk_caps(&c)),
            Type::Bounded(t, c) => Type::Bounded(z(&t), self.zonk_caps(&c)),
            t @ (Type::Var(_) | Type::Con(_)) => t,
            Type::App(h, a) => Type::App(z(&h), z(&a)),
            Type::Fun(p, s, r) => Type::Fun(z(&p), self.zonk_stage(&s), z(&r)),
            Type::Eff(r, e, t) => Type::Eff(self.zonk_row(&r), self.zonk_row(&e), z(&t)),
            Type::EffOf(r, e) => Type::EffOf(self.zonk_row(&r), self.zonk_row(&e)),
            Type::Ref(t) => Type::Ref(z(&t)),
            Type::Layer(i, e, o) => {
                Type::Layer(self.zonk_row(&i), self.zonk_row(&e), self.zonk_row(&o))
            }
            Type::Record(f) => Type::Record(self.zonk_fields(&f)),
            Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| self.zonk(t)).collect()),
            Type::Errors(r) => Type::Errors(self.zonk_row(&r)),
            Type::Impl(r) => Type::Impl(self.zonk_row(&r)),
            Type::Tag(r) => Type::Tag(self.zonk_row(&r)),
            Type::Dict(c, ts) => Type::Dict(c, ts.iter().map(|t| self.zonk(t)).collect()),
            Type::Env(r) => Type::Env(self.zonk_row(&r)),
            Type::Forall(s) => Type::Forall(Rc::new(Scheme {
                vars: s.vars.clone(),
                body: self.zonk(&s.body),
                outlives: s.outlives.clone(),
            })),
        }
    }

    pub(super) fn zonk_stage(&self, s: &Stage) -> Stage {
        match s {
            Stage::Pure => Stage::Pure,
            Stage::Runtime(r, e) => Stage::Runtime(self.zonk_row(r), self.zonk_row(e)),
        }
    }

    pub(super) fn zonk_row(&self, r: &Row) -> Row {
        let mut out = Row {
            elems: r
                .elems
                .iter()
                .map(|(n, args)| (n.clone(), args.iter().map(|a| self.zonk(a)).collect()))
                .collect(),
            vars: BTreeSet::new(),
        };
        for v in &r.vars {
            match self.solution(v) {
                Some(Solved::Row(inner)) => {
                    let inner = self.zonk_row(inner);
                    for (n, args) in inner.elems {
                        out.elems.entry(n).or_insert(args);
                    }
                    out.vars.extend(inner.vars);
                }
                _ => {
                    out.vars.insert(v.clone());
                }
            }
        }
        drop_empty_live(&mut out);
        out
    }

    pub(super) fn zonk_fields(&self, f: &Fields) -> Fields {
        let mut fields: BTreeMap<Name, Type> = f
            .fields
            .iter()
            .map(|(l, t)| (l.clone(), self.zonk(t)))
            .collect();
        let mut tail = f.tail.clone();
        while let Some(v) = &tail
            && let Some(Solved::Fields(more)) = self.solution(v)
        {
            for (l, t) in &more.fields {
                fields.insert(l.clone(), self.zonk(t));
            }
            tail = more.tail.clone();
        }
        Fields { fields, tail }
    }

    /// An opaque alias of the module being checked, applied to all its
    /// parameters: its name and arguments.
    pub(super) fn alias<'t>(&self, t: &'t Type) -> Option<(&'t Path, Vec<&'t Type>)> {
        if self.decls.opaque.is_empty() {
            return None;
        }
        let mut args = Vec::new();
        let mut head = t;
        while let Type::App(h, a) = head {
            args.push(&**a);
            head = h;
        }
        let Type::Con(p) = head else {
            return None;
        };
        let (vars, _) = self
            .decls
            .opaque
            .get(p)
            .filter(|_| p.module == self.module)?;
        if vars.len() != args.len() {
            return None;
        }
        args.reverse();
        Some((p, args))
    }

    /// An opaque alias of the module being checked, applied to all its
    /// parameters: its representation, each argument at its kind.
    fn unfold(&self, t: &Type) -> Option<Type> {
        let (p, args) = self.alias(t)?;
        let (vars, body) = &self.decls.opaque[p];
        let map = vars
            .iter()
            .cloned()
            .zip(args.into_iter().map(arg))
            .collect();
        Some(substitute(body, &map))
    }
}
