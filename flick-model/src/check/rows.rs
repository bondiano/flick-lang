//! Stages and subtypes (`docs/formal/core.md` §2.1, §2.2): `φ ⊑ φ'` is
//! `ρ ⊆ ρ'` on both rows, `≤` lets a stage and rows of an arrow or `Eff`
//! grow and the parameter shrink, data its arguments by their signs
//! and a record or tuple its fields. A row that is still a meta when an
//! inclusion needs it waits; once nothing more is known each waiting meta
//! is the least row, `none`: the least solution of §2.1.

use std::mem;
use std::rc::Rc;
use std::slice;

use super::regions::{add_live, arg_caps, live_arg, live_stage};
use super::{Check, Checker, error, live_service};
use crate::syntax::Path;
use crate::types::{Caps, PROVIDED, Row, Stage, Type, Var};
use crate::variance::Sign;

impl Checker<'_> {
    /// `ρ ⊆ ρ'`. An element or a rigid variable `ρ'` lacks goes into its
    /// meta, if it has one; a meta of `ρ` waits.
    pub(super) fn within(&mut self, a: &Row, b: &Row) -> Check<()> {
        let (a, b) = (self.zonk_row(a), self.zonk_row(b));
        let mut extra = Row::none();
        let scoped = self.scope_live(&b);
        for (name, args) in &a.elems {
            match b.elems.get(name) {
                // `{Live C} ⊆ {Live C'}` is `C ⊆ C'`, which may grow `C'`;
                // `Scope ϱ` of `ρ'` keeps `ϱ` live too
                Some(theirs) if *name == live_service() => {
                    let allowed = arg_caps(&theirs[0]).union(&scoped);
                    self.hold(slice::from_ref(&args[0]), &[], &allowed, "Live");
                }
                // against a closed row, `Live C` needs `C` to hold no region
                // or variable but those its `Scope` keeps live, once known:
                // what is unknown now may be none
                None if *name == live_service()
                    && !b.vars.iter().any(|v| matches!(v, Var::Meta(_))) =>
                {
                    self.hold(slice::from_ref(&args[0]), &[], &scoped, "Live");
                }
                Some(theirs) if theirs.len() == args.len() => {
                    for (x, y) in args.iter().zip(theirs.iter()) {
                        self.unify(x, y)?;
                    }
                }
                Some(_) => return error("⊆", format!("`{name}` with other arguments")),
                None => {
                    extra.elems.insert(name.clone(), args.clone());
                }
            }
        }
        for v in &a.vars {
            let held = b.vars.contains(v);
            match v {
                // what `ρ'` takes out of `ρ` it has to name itself
                Var::Named(_) if held => {
                    for n in a.owed(v, &b) {
                        extra.elems.insert(n.clone(), Rc::from([]));
                    }
                }
                Var::Named(_) => extra.add_var(v, &a.less_of(v).cloned().collect()),
                Var::Meta(_) if held && a.owed(v, &b).next().is_none() => {}
                Var::Meta(_) => self.pending.push((a.only(v), b.clone())),
            }
        }
        if extra.is_none() {
            return Ok(());
        }
        let tail = b.vars.iter().find_map(|v| match v {
            Var::Meta(m) => Some(*m),
            Var::Named(_) => None,
        });
        // a row taken without `S` does not grow by `S`, nor by a variable
        // that may hold it
        let closed = |m: u32| {
            let m = Var::Meta(m);
            b.less_of(&m).any(|n| {
                extra.elems.contains_key(n)
                    || (extra.vars.iter()).any(|v| !extra.less.contains(&(v.clone(), n.clone())))
            })
        };
        match tail.filter(|m| !closed(*m)) {
            Some(m) => {
                let mut grown = extra;
                grown.vars.insert(Var::Meta(self.meta()));
                self.bind_row(m, grown)
            }
            None => error("⊆", format!("the row `{b}` does not allow `{extra}`")),
        }
    }

    /// Solves the waiting inclusions; a meta nothing put a row into is
    /// `none`.
    pub(super) fn settle(&mut self) -> Check<()> {
        loop {
            let pending = mem::take(&mut self.pending);
            if pending.is_empty() {
                return Ok(());
            }
            let mut waiting = Vec::new();
            let mut progressed = false;
            for (a, b) in pending {
                let z = self.zonk_row(&a);
                let unsolved =
                    z.elems.is_empty() && z.vars.iter().all(|v| matches!(v, Var::Meta(_)));
                if unsolved && z == a {
                    waiting.push((a, b));
                } else {
                    progressed = true;
                    self.within(&z, &b)?;
                }
            }
            if !progressed {
                for (a, _) in &waiting {
                    for v in &self.zonk_row(a).vars {
                        if let Var::Meta(m) = v {
                            self.bind_row(*m, Row::none())?;
                        }
                    }
                }
            }
            self.pending.extend(waiting);
        }
    }

    /// `ρ + ρ'`, with the arguments of an element in both made equal.
    pub(super) fn sum(&mut self, a: &Row, b: &Row) -> Check<Row> {
        let (a, b) = (self.zonk_row(a), self.zonk_row(b));
        let mut out = a.clone();
        for (name, args) in &b.elems {
            match a.elems.get(name) {
                Some(mine) if *name == live_service() => {
                    let both = arg_caps(&mine[0]).union(&arg_caps(&args[0]));
                    out.elems.insert(name.clone(), Rc::from([live_arg(both)]));
                }
                Some(mine) if &*name.module == PROVIDED => {
                    let owner = self.both_owners(&mine[0], &args[0])?;
                    out.elems.insert(name.clone(), Rc::from([owner]));
                }
                Some(mine) if mine.len() == args.len() => {
                    for (x, y) in mine.iter().zip(args.iter()) {
                        self.unify(x, y)?;
                    }
                }
                Some(_) => return error("+", format!("`{name}` twice with other arguments")),
                None => {
                    out.elems.insert(name.clone(), args.clone());
                }
            }
        }
        for v in &b.vars {
            out.add_var(v, &b.less_of(v).cloned().collect());
        }
        Ok(out)
    }

    /// `φ ⊔ φ'` into the stage of the expression being checked.
    pub(super) fn perform(&mut self, s: &Stage) -> Check<()> {
        let s = &match s {
            Stage::Runtime(r, e) => Stage::Runtime(self.split_provided(r)?, e.clone()),
            Stage::Pure => Stage::Pure,
        };
        self.raised(s);
        self.lexical_owner(s)?;
        self.stage = match (self.stage.clone(), s) {
            (Stage::Pure, s) => self.zonk_stage(s),
            (current, Stage::Pure) => current,
            (Stage::Runtime(r, e), Stage::Runtime(r2, e2)) => {
                Stage::Runtime(self.sum(&r, r2)?, self.sum(&e, e2)?)
            }
        };
        Ok(())
    }

    /// `φ ⊑ φ'`: `R⟨…⟩ ⋢ P` (`FLICK-E001`), and rows include.
    pub(super) fn stage_within(&mut self, a: &Stage, b: &Stage) -> Check<()> {
        match (a, b) {
            (Stage::Pure, _) => Ok(()),
            (Stage::Runtime(..), Stage::Pure) => error(
                "⊑",
                format!(
                    "a runtime step `{}` where only a pure one is allowed",
                    self.zonk_stage(a)
                ),
            ),
            (Stage::Runtime(r, e), Stage::Runtime(r2, e2)) => {
                let mut r = self.split_provided(r)?;
                self.settle_given(&mut r);
                self.within(&r, r2)?;
                self.within(e, e2)
            }
        }
    }

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
        if matches!(b, Type::Var(Var::Meta(_))) {
            return self.unify(&a, &b);
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
        // each qualifier of the expected type bounds the `Live` of its use
        let (qa, mut qes) = (self.qualifier(actual), self.qualifiers(expected));
        if qes.is_empty() {
            qes.push(Caps::default());
        }
        match (self.resolve(actual), self.resolve(expected)) {
            (Type::Fun(p, s, r), Type::Fun(p2, s2, r2)) => {
                self.subsume(&p2, &p)?;
                let s = live_stage(&s, &qa);
                for qe in &qes {
                    self.stage_within(&s, &live_stage(&s2, qe))?;
                }
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
                let t = self.instantiate(&s);
                self.subsume(&t, &other)
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
            (a, b) => self.unify(&a, &b),
        }
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

    /// `T ū` as `T` and `ū`, metas at the heads followed.
    fn data_args(&self, t: &Type) -> Option<(Path, Vec<Type>)> {
        let mut args = Vec::new();
        let mut head = self.heads(t);
        while let Type::App(h, x) = head {
            args.push((*x).clone());
            head = self.heads(&h);
        }
        args.reverse();
        match head {
            Type::Con(p) => Some((p, args)),
            _ => None,
        }
    }

    /// `ρ` without what `of` has: the services `provide` leaves its
    /// surroundings to give. A variable of `ρ` is left without the names of
    /// `of`, which it may hold.
    pub(super) fn minus(&mut self, r: &Row, of: &Row) -> Check<Row> {
        let (r, of) = (self.zonk_row(r), self.zonk_row(of));
        let mut out = Row::none();
        for (name, args) in &r.elems {
            match of.elems.get(name) {
                Some(theirs) => {
                    for (x, y) in args.iter().zip(theirs.iter()) {
                        self.unify(x, y)?;
                    }
                }
                None => {
                    out.elems.insert(name.clone(), args.clone());
                }
            }
        }
        let named = (of.elems.iter()).filter(|(_, args)| args.is_empty());
        let gone: Vec<&Path> = named.map(|(n, _)| n).collect();
        // a handler finds an instance of a service with arguments by its
        // name: a variable that may hold another instance would get this one
        for v in r.vars.difference(&of.vars) {
            let instance = (of.elems.iter())
                .find(|(n, args)| !args.is_empty() && !r.less_of(v).any(|l| l == *n));
            if let Some((n, _)) = instance {
                return error(
                    "provide",
                    format!("`{v:?}` may hold `{n}` at other arguments"),
                );
            }
        }
        for v in r.vars.difference(&of.vars) {
            out.add_var(
                v,
                &r.less_of(v).chain(gone.iter().copied()).cloned().collect(),
            );
        }
        Ok(out)
    }
}
