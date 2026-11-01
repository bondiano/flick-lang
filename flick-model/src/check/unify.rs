//! Metas and `≡` (`docs/formal/core.md` §1.2): unification of types and
//! record fields, `Errors {E} ≡ E` and `Errors none ≡ Never`, and
//! `EffOf ρ ρ' τ ≡ Eff ρ ρ' τ` (§1.6).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::generalize::{Inst, rigid_inst, substitute_all};
use super::{Check, Checker, Slot, Solved, error};
use crate::syntax::{Name, Path};
use crate::types::{Fields, Row, Scheme, Stage, Type, Var};

impl Checker<'_> {
    pub(super) fn meta(&mut self) -> u32 {
        self.slots.push(Slot {
            level: self.level,
            solved: None,
        });
        u32::try_from(self.slots.len() - 1).expect("fewer metas than `u32::MAX`")
    }

    pub(super) fn fresh(&mut self) -> Type {
        Type::Var(Var::Meta(self.meta()))
    }

    pub(super) fn fresh_row(&mut self) -> Row {
        Row::var(Var::Meta(self.meta()))
    }

    fn solution(&self, v: &Var) -> Option<&Solved> {
        match v {
            Var::Meta(m) => self.slots[*m as usize].solved.as_ref(),
            Var::Named(_) => None,
        }
    }

    /// `t` with the metas at its head followed, and `EffOf ρ ρ' τ` as
    /// `Eff ρ ρ' τ`. A qualifier is not yet checked: the type under it.
    pub(super) fn resolve(&self, t: &Type) -> Type {
        let mut t = t.clone();
        loop {
            match &t {
                Type::Var(v) => match self.solution(v) {
                    Some(Solved::Type(next)) => t = next.clone(),
                    _ => break,
                },
                Type::Captured(inner, _) | Type::Bounded(inner, _) => t = (**inner).clone(),
                _ => break,
            }
        }
        if let Type::App(h, a) = &t
            && let Type::EffOf(r, e) = self.resolve(h)
        {
            return Type::Eff(r, e, a.clone());
        }
        t
    }

    /// `t` with every solved meta replaced.
    pub(super) fn zonk(&self, t: &Type) -> Type {
        let z = |t: &Rc<Type>| Rc::new(self.zonk(t));
        match self.resolve(t) {
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
            Type::Captured(..) | Type::Bounded(..) => unreachable!("resolved"),
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

    /// The metas of `t`, with the level each is at.
    pub(super) fn metas(&self, t: &Type, out: &mut BTreeSet<u32>) {
        let row = |r: &Row, out: &mut BTreeSet<u32>| {
            for v in &r.vars {
                if let Var::Meta(m) = v {
                    out.insert(*m);
                }
            }
            for args in r.elems.values() {
                args.iter().for_each(|a| self.metas(a, out));
            }
        };
        match self.zonk(t) {
            Type::Var(Var::Meta(m)) => {
                out.insert(m);
            }
            Type::Var(_) | Type::Con(_) => {}
            Type::App(h, a) => {
                self.metas(&h, out);
                self.metas(&a, out);
            }
            Type::Fun(p, s, r) => {
                self.metas(&p, out);
                if let Stage::Runtime(a, b) = &s {
                    row(a, out);
                    row(b, out);
                }
                self.metas(&r, out);
            }
            Type::Eff(r, e, t) => {
                row(&r, out);
                row(&e, out);
                self.metas(&t, out);
            }
            Type::EffOf(r, e) => [r, e].iter().for_each(|r| row(r, out)),
            Type::Ref(t) => self.metas(&t, out),
            Type::Layer(i, e, o) => [i, e, o].iter().for_each(|r| row(r, out)),
            Type::Record(f) => {
                f.fields.values().for_each(|t| self.metas(t, out));
                if let Some(Var::Meta(m)) = f.tail {
                    out.insert(m);
                }
            }
            Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().for_each(|t| self.metas(t, out)),
            Type::Errors(r) | Type::Impl(r) | Type::Tag(r) | Type::Env(r) => row(&r, out),
            Type::Forall(s) => self.metas(&s.body, out),
            Type::Captured(..) | Type::Bounded(..) => unreachable!("zonked"),
        }
    }

    /// Lowers the level of every meta in `t` to `level`: what a meta at
    /// that level is solved to is reachable from there.
    fn lower(&mut self, t: &Type, level: u32) {
        let mut ms = BTreeSet::new();
        self.metas(t, &mut ms);
        for m in ms {
            let slot = &mut self.slots[m as usize];
            slot.level = slot.level.min(level);
        }
    }

    fn bind(&mut self, m: u32, solved: Solved, occurs: &Type) -> Check<()> {
        let mut ms = BTreeSet::new();
        self.metas(occurs, &mut ms);
        if ms.contains(&m) {
            return error(
                "≡",
                format!("an infinite type: ?{m} in {}", self.zonk(occurs)),
            );
        }
        let level = self.slots[m as usize].level;
        self.lower(occurs, level);
        self.slots[m as usize].solved = Some(solved);
        Ok(())
    }

    pub(super) fn bind_row(&mut self, m: u32, r: Row) -> Check<()> {
        self.bind(m, Solved::Row(r.clone()), &Type::Errors(r))
    }

    fn bind_fields(&mut self, m: u32, f: Fields) -> Check<()> {
        self.bind(m, Solved::Fields(f.clone()), &Type::Record(f))
    }

    pub(super) fn mismatch<T>(&self, a: &Type, b: &Type) -> Check<T> {
        error("≡", format!("`{}` is not `{}`", self.zonk(a), self.zonk(b)))
    }

    /// `τ ≡ τ'`.
    pub(super) fn unify(&mut self, a: &Type, b: &Type) -> Check<()> {
        let (a, b) = (self.resolve(a), self.resolve(b));
        match (&a, &b) {
            (Type::Var(Var::Meta(m)), Type::Var(Var::Meta(n))) if m == n => Ok(()),
            (Type::Var(Var::Meta(m)), t) | (t, Type::Var(Var::Meta(m))) => {
                self.bind(*m, Solved::Type(t.clone()), t)
            }
            (Type::Var(x), Type::Var(y)) if x == y => Ok(()),
            (Type::Con(p), Type::Con(q)) if p == q => Ok(()),
            (Type::App(h, x), Type::App(g, y)) => {
                self.unify(h, g)?;
                self.unify(x, y)
            }
            (Type::Fun(p, s, r), Type::Fun(q, t, u)) => {
                self.unify(p, q)?;
                self.unify_stages(s, t)?;
                self.unify(r, u)
            }
            (Type::Eff(r, e, t), Type::Eff(r2, e2, t2)) => {
                self.unify_rows(r, r2)?;
                self.unify_rows(e, e2)?;
                self.unify(t, t2)
            }
            (Type::App(h, x), Type::Eff(r, e, t)) | (Type::Eff(r, e, t), Type::App(h, x)) => {
                self.unify(h, &Type::EffOf(r.clone(), e.clone()))?;
                self.unify(x, t)
            }
            (Type::EffOf(r, e), Type::EffOf(r2, e2)) => {
                self.unify_rows(r, r2)?;
                self.unify_rows(e, e2)
            }
            (Type::Ref(x), Type::Ref(y)) => self.unify(x, y),
            (Type::Layer(i, e, o), Type::Layer(i2, e2, o2)) => {
                self.unify_rows(i, i2)?;
                self.unify_rows(e, e2)?;
                self.unify_rows(o, o2)
            }
            (Type::Record(f), Type::Record(g)) => self.unify_fields(f, g),
            (Type::Impl(r), t @ Type::Record(_)) | (t @ Type::Record(_), Type::Impl(r)) => {
                let ops = Type::Record(self.implementation(r)?);
                self.unify(&ops, t)
            }
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys.iter())
                .try_for_each(|(x, y)| self.unify(x, y)),
            (Type::Errors(r), Type::Errors(s))
            | (Type::Impl(r), Type::Impl(s))
            | (Type::Tag(r), Type::Tag(s))
            | (Type::Env(r), Type::Env(s)) => self.unify_rows(r, s),
            (Type::Errors(r), t) | (t, Type::Errors(r)) => self.project(r, t),
            (Type::Dict(c, xs), Type::Dict(d, ys)) if c == d && xs.len() == ys.len() => xs
                .iter()
                .zip(ys.iter())
                .try_for_each(|(x, y)| self.unify(x, y)),
            (Type::Forall(s), Type::Forall(t)) if s.vars.len() == t.vars.len() => {
                let names: Vec<Inst> = s
                    .vars
                    .iter()
                    .map(|(_, k)| {
                        self.rigid += 1;
                        rigid_inst(&format!("#{}", self.rigid), k)
                    })
                    .collect();
                let x = substitute_all(&s.body, &s.vars, &names);
                let y = substitute_all(&t.body, &t.vars, &names);
                self.unify(&x, &y)
            }
            _ => self.mismatch(&a, &b),
        }
    }

    pub(super) fn unify_stages(&mut self, s: &Stage, t: &Stage) -> Check<()> {
        match (s, t) {
            (Stage::Pure, Stage::Pure) => Ok(()),
            (Stage::Runtime(r, e), Stage::Runtime(r2, e2)) => {
                self.unify_rows(r, r2)?;
                self.unify_rows(e, e2)
            }
            _ => error(
                "≡",
                format!("the stages `{s}` and `{t}` differ: a runtime arrow is no pure one"),
            ),
        }
    }

    /// `ρ ≡ ρ'`: a bare meta is solved at once, else both inclusions.
    pub(super) fn unify_rows(&mut self, a: &Row, b: &Row) -> Check<()> {
        let (a, b) = (self.zonk_row(a), self.zonk_row(b));
        if a == b {
            return Ok(());
        }
        for (x, y) in [(&a, &b), (&b, &a)] {
            if x.elems.is_empty()
                && let [Var::Meta(m)] = x.vars.iter().collect::<Vec<_>>()[..]
            {
                return self.bind_row(*m, y.clone());
            }
        }
        self.within(&a, &b)?;
        self.within(&b, &a)
    }

    fn unify_fields(&mut self, a: &Fields, b: &Fields) -> Check<()> {
        let (a, b) = (self.zonk_fields(a), self.zonk_fields(b));
        for (l, t) in &a.fields {
            if let Some(u) = b.fields.get(l) {
                self.unify(t, u)?;
            }
        }
        let only = |x: &Fields, y: &Fields| Fields {
            fields: x
                .fields
                .iter()
                .filter(|(l, _)| !y.fields.contains_key(*l))
                .map(|(l, t)| (l.clone(), t.clone()))
                .collect(),
            tail: None,
        };
        let (a_only, b_only) = (only(&a, &b), only(&b, &a));
        let missing = |f: &Fields| f.fields.keys().next().cloned();
        match (&a.tail, &b.tail) {
            (Some(Var::Meta(m)), Some(Var::Meta(n))) if m == n => {
                match missing(&a_only).or(missing(&b_only)) {
                    None => Ok(()),
                    Some(l) => error("Rec", format!("field `{l}` is in one record only")),
                }
            }
            (Some(Var::Meta(m)), Some(Var::Meta(n))) => {
                let tail = Var::Meta(self.meta());
                self.bind_fields(
                    *m,
                    Fields {
                        tail: Some(tail.clone()),
                        ..b_only
                    },
                )?;
                self.bind_fields(
                    *n,
                    Fields {
                        tail: Some(tail),
                        ..a_only
                    },
                )
            }
            (Some(Var::Meta(m)), tail) if a_only.fields.is_empty() => self.bind_fields(
                *m,
                Fields {
                    tail: tail.clone(),
                    ..b_only
                },
            ),
            (tail, Some(Var::Meta(n))) if b_only.fields.is_empty() => self.bind_fields(
                *n,
                Fields {
                    tail: tail.clone(),
                    ..a_only
                },
            ),
            (x, y) if x == y => match missing(&a_only).or(missing(&b_only)) {
                None => Ok(()),
                Some(l) => error(
                    "Rec",
                    format!("field `{l}` is in one record only: records are exact"),
                ),
            },
            _ => self.mismatch(&Type::Record(a.clone()), &Type::Record(b.clone())),
        }
    }

    /// `Errors ρ ≡ τ` for `τ` not a row projection: `Errors {E} ≡ E`,
    /// `Errors none ≡ Never` (§1.5); an unknown `ρ` becomes that row.
    fn project(&mut self, r: &Row, t: &Type) -> Check<()> {
        let r = self.zonk_row(r);
        let never = Type::con("Prelude", "Never");
        let elems: Vec<_> = r.elems.iter().collect();
        let vars: Vec<_> = r.vars.iter().collect();
        match (elems.as_slice(), vars.as_slice()) {
            ([(e, args)], []) => self.unify(&error_type(e, args), t),
            ([], []) => self.unify(&never, t),
            ([], [Var::Meta(m)]) => {
                let t = self.zonk(t);
                if t == never {
                    return self.bind_row(*m, Row::none());
                }
                match self.error_of(&t) {
                    Some((e, args)) => {
                        let mut row = Row::none();
                        row.elems.insert(e, args);
                        self.bind_row(*m, row)
                    }
                    None => self.mismatch(&Type::Errors(r.clone()), &t),
                }
            }
            _ => self.mismatch(&Type::Errors(r.clone()), t),
        }
    }

    /// `E ῡ` as the element of an error row, if `t` is a declared error.
    pub(super) fn error_of(&self, t: &Type) -> Option<(Path, Rc<[Type]>)> {
        let mut args = Vec::new();
        let mut head = self.zonk(t);
        while let Type::App(h, a) = head {
            args.push((*a).clone());
            head = (*h).clone();
        }
        args.reverse();
        match head {
            Type::Con(p) if self.decls.errors.contains(&p) => Some((p, args.into())),
            _ => None,
        }
    }
}

/// `E ῡ` as a type.
pub(super) fn error_type(e: &Path, args: &[Type]) -> Type {
    args.iter()
        .fold(Type::Con(e.clone()), |h, a| Type::app(h, a.clone()))
}
