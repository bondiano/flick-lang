//! Subtyping `τ ≤ τ'` (`docs/formal/core.md` §2.2, `docs/formal/regions.md` §2.2).

use std::slice;

use super::generalize::{Inst, names as inst_names, rigid_inst, substitute_all, substitute_caps};
use super::regions::{add_live, live_arg, live_stage};
use super::{Check, Checker};
use crate::syntax::Path;
use crate::types::{Caps, Kind, Scheme, Stage, Type, Var};
use crate::variance::Sign;

impl Checker<'_> {
    /// `τ ≤ τ'`: ≤-Fun and ≤-Eff, else ≤-Refl; with (≤-Cap) of
    /// `regions.md` §2.2 on the whole value, qualified or bare.
    pub(super) fn subsume(&mut self, actual: &Type, expected: &Type) -> Check<()> {
        // an opaque alias on both sides met again inside its own unfolding
        // is equal by its arguments, as `unify` reads it: unfolding one that
        // names itself would not end
        let (x, y) = (self.heads(actual), self.heads(expected));
        let alias = match (self.alias(&x), self.alias(&y)) {
            (Some((p, _)), Some((q, _))) if p == q => Some(p.clone()),
            _ => None,
        };
        let Some(p) = alias else {
            return self.subsume_unfolded(actual, expected);
        };
        if self.unfolding.contains(&p) {
            return self.unify(&x, &y);
        }
        self.unfolding.push(p);
        let checked = self.subsume_unfolded(actual, expected);
        self.unfolding.pop();
        checked
    }

    fn subsume_unfolded(&mut self, actual: &Type, expected: &Type) -> Check<()> {
        let (a, b) = (self.follow(actual), self.follow(expected));
        // An unknown side takes the complete type, including its qualifier.
        // Erasing it here loses captures through an inferred eta parameter.
        let unknown_actual = matches!(&a, Type::Var(v @ Var::Meta(_)) if !self.mentions(v, &b));
        if unknown_actual {
            return self.unify(&a, &b);
        }
        if matches!(b, Type::Var(Var::Meta(_))) {
            let widened = self.widen_for_meta(&a);
            return self.unify(&widened, &b);
        }
        self.subsume_bare(actual, expected)?;
        // the hidden row of data with hidden storage only widens: a region
        // its other arguments show does not stand in for it, the variables
        // they show do (`regions.md` §2.2)
        let rows = self.bare_hidden(&self.resolve(&a));
        let row = |c: Vec<Caps>| live_arg(c.iter().fold(Caps::default(), |o, c| o.union(c)));
        for b in self.layers(&b) {
            if rows {
                let (held, within) = (row(self.qualifiers(&a)), row(self.qualifiers(&b)));
                let mut shown = self.reach(&b);
                shown.atoms.retain(|v| !self.is_region(v));
                shown.vars.clear();
                self.hold(&[held], &[within], &shown, "≤-Cap");
            } else {
                self.hold(slice::from_ref(&a), &[b], &Caps::default(), "≤-Cap");
            }
        }
        Ok(())
    }

    fn subsume_bare(&mut self, actual: &Type, expected: &Type) -> Check<()> {
        // under `regions/live.rs` a qualifier on a function or a task is
        // the `Live` its use needs
        // each hidden qualifier of the expected type bounds the `Live` of its
        // use; a value bound allows no use
        let (qa, mut qes) = (self.qualifier(actual), self.hidden_qualifiers(expected));
        if qes.is_empty() {
            qes.push(Caps::default());
        }
        match (self.resolve(actual), self.resolve(expected)) {
            (Type::Fun(p, s, r), Type::Fun(p2, s2, r2)) => {
                self.subsume(&p2, &p)?;
                let s_live = live_stage(&s, &qa);
                for qe in &qes {
                    self.stage_within(&s_live, &live_stage(&s2, qe))?;
                }
                let r = if s == Stage::Pure && self.is_callable(&r) {
                    self.rewrap_bounds(actual, (*r).clone())
                } else {
                    (*r).clone()
                };
                self.subsume(&r, &r2)
            }
            (Type::Eff(r, e, t), Type::Eff(r2, e2, t2)) => {
                let r = add_live(&r, &qa);
                for qe in &qes {
                    self.within(&r, &add_live(&r2, qe))?;
                }
                self.within(&e, &e2)?;
                self.subsume(&t, &t2)
            }
            (Type::Forall(s), other) if !matches!(other, Type::Forall(_)) => {
                let inst = self.instantiate(&s);
                let t = self.rewrap_qualifiers(actual, inst);
                self.subsume(&t, expected)
            }
            (Type::Forall(s), Type::Forall(t)) => {
                if s.vars.len() != t.vars.len()
                    || !s
                        .vars
                        .iter()
                        .zip(&t.vars)
                        .all(|((_, k1), (_, k2))| k1 == k2)
                {
                    return self.mismatch(actual, expected);
                }
                self.subsume_forall(actual, &s, expected, &t)
            }
            // a record and a tuple are immutable: field by field (≤-Rec)
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys.iter()) {
                    self.subsume(x, y)?;
                }
                Ok(())
            }
            (Type::Record(f), Type::Record(g))
                if f.tail.is_none() && g.tail.is_none() && f.fields.keys().eq(g.fields.keys()) =>
            {
                for (x, y) in f.fields.values().zip(g.fields.values()) {
                    self.subsume(x, y)?;
                }
                Ok(())
            }
            (a @ Type::App(..), b @ Type::App(..)) => self.subsume_data(&a, &b),
            (Type::Dict(c, xs), Type::Dict(d, ys)) if c == d && xs.len() == ys.len() => {
                self.subsume_dict(&c, &xs, &ys)
            }
            // a function or a task that a bounded variable takes may need the
            // innermost bound, the capacity of a cell `Ref.make` makes: its
            // type records it, as the bound allows its use nothing more; and
            // closed service/error rows widen so a later value needing a
            // service or error fits in either order
            (Type::Fun(p, s, r), b @ Type::Var(Var::Meta(_))) => {
                let s = self
                    .innermost_bound(expected)
                    .map_or(s.clone(), |c| live_stage(&s, &c));
                let widened = self.widen_for_meta(&Type::Fun(p, s, r));
                self.unify(&widened, &b)
            }
            (Type::Eff(r, e, t), b @ Type::Var(Var::Meta(_))) => {
                let r = self
                    .innermost_bound(expected)
                    .map_or(r.clone(), |c| add_live(&r, &c));
                let widened = self.widen_for_meta(&Type::Eff(r, e, t));
                self.unify(&widened, &b)
            }
            (a, b @ Type::Var(Var::Meta(_))) if !matches!(&a, Type::Var(Var::Meta(_))) => {
                let widened = self.widen_for_meta(&a);
                self.unify(&widened, &b)
            }
            (a, b) => self.unify(&a, &b),
        }
    }

    fn subsume_forall(
        &mut self,
        actual: &Type,
        s: &Scheme,
        expected: &Type,
        t: &Scheme,
    ) -> Check<()> {
        let names: Vec<Inst> = s
            .vars
            .iter()
            .map(|(_, k)| {
                self.rigid += 1;
                let name = format!("#{}", self.rigid);
                if *k == Kind::Region {
                    self.regions.kinds.insert(Var::Named(name.clone().into()));
                }
                rigid_inst(&name, k)
            })
            .collect();
        let facts = self.regions.facts.len();
        let map_t = inst_names(&t.vars, &names);
        for (c, r) in &t.outlives {
            self.regions
                .facts
                .push((substitute_caps(c, &map_t), super::substitute(r, &map_t)));
        }
        let map_s = inst_names(&s.vars, &names);
        for (c, r) in &s.outlives {
            self.need_outlives(substitute_caps(c, &map_s), super::substitute(r, &map_s));
        }
        let x = self.rewrap_qualifiers(actual, substitute_all(&s.body, &s.vars, &names));
        let y = self.rewrap_qualifiers(expected, substitute_all(&t.body, &t.vars, &names));
        let result = self.subsume(&x, &y);
        self.regions.facts.truncate(facts);
        result
    }

    /// `T ū ≤ T ū'` by the sign of each parameter (≤-Data, `regions.md`
    /// §2.2.1): a `−` argument narrows and keeps what it reaches.
    fn subsume_data(&mut self, a: &Type, b: &Type) -> Check<()> {
        let (Some((p, xs)), Some((q, ys))) = (self.data_args(a), self.data_args(b)) else {
            return self.unify(a, b);
        };
        let signs = (self.decls.signs.get(&p))
            .filter(|s| p == q && s.len() == xs.len() && xs.len() == ys.len())
            .cloned();
        let Some(signs) = signs else {
            return self.unify(a, b);
        };
        for ((x, y), s) in xs.iter().zip(&ys).zip(signs) {
            // an argument inference has yet to solve takes the other as it
            // is, its bound too, as `≡` of the whole type did
            let open = |t: &Type| matches!(self.resolve(t), Type::Var(Var::Meta(_)));
            match s {
                Sign::Pos if !open(x) && open(y) => {
                    let widened = self.widen_for_meta(x);
                    self.unify(&widened, y)?;
                }
                _ if open(x) || open(y) => self.unify(x, y)?,
                Sign::Pos => self.subsume(x, y)?,
                Sign::Neg => {
                    self.subsume(y, x)?;
                    self.hold(
                        slice::from_ref(x),
                        slice::from_ref(y),
                        &Caps::default(),
                        "≤-Data",
                    );
                }
                Sign::Inv | Sign::Bi => self.unify(x, y)?,
            }
        }
        Ok(())
    }

    /// `D_C ū ≤ D_C ū'` by the sign of each class parameter (`regions.md`
    /// §2.2.1, Lean `LeLive.dict`): `+` widens covariantly, `−` narrows
    /// contravariantly while keeping reach, and `inv`/`bi` require `≡`.
    fn subsume_dict(&mut self, c: &Path, xs: &[Type], ys: &[Type]) -> Check<()> {
        let signs = self
            .decls
            .class_signs
            .get(c)
            .filter(|s| s.len() == xs.len() && xs.len() == ys.len())
            .cloned();
        let Some(signs) = signs else {
            for (x, y) in xs.iter().zip(ys) {
                self.unify(x, y)?;
            }
            return Ok(());
        };
        for ((x, y), s) in xs.iter().zip(ys).zip(signs) {
            let open = |t: &Type| matches!(self.resolve(t), Type::Var(Var::Meta(_)));
            match s {
                Sign::Pos if !open(x) && open(y) => {
                    let widened = self.widen_for_meta(x);
                    self.unify(&widened, y)?;
                }
                _ if open(x) || open(y) => self.unify(x, y)?,
                Sign::Pos => self.subsume(x, y)?,
                Sign::Neg => {
                    self.subsume(y, x)?;
                    self.hold(
                        slice::from_ref(x),
                        slice::from_ref(y),
                        &Caps::default(),
                        "≤-Dict",
                    );
                }
                Sign::Inv | Sign::Bi => self.unify(x, y)?,
            }
        }
        Ok(())
    }
}
