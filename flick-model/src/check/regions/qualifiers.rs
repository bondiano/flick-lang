//! Qualifiers, value-bound layers, and stage closure checks on callables.

use std::rc::Rc;

use super::live::live_stage;
use crate::check::{Check, Checker};
use crate::types::{Caps, Stage, Type};

impl Checker<'_> {
    /// Whether `t` is a function, polymorphic callable or task whose qualifier
    /// tracks what using it needs live.
    pub(in crate::check) fn is_callable(&self, t: &Type) -> bool {
        matches!(
            self.resolve(t),
            Type::Fun(..) | Type::Forall(_) | Type::Eff(..)
        )
    }

    /// Wraps `inner` with the outer qualifiers of `outer`, unioning hidden
    /// qualifiers (`Type::Captured`) without stacking duplicate layers and
    /// preserving distinct value bounds (`Type::Bounded`).
    pub(in crate::check) fn rewrap_qualifiers(&self, outer: &Type, inner: Type) -> Type {
        let mut layers = Vec::new();
        let mut cur = self.follow(outer);
        loop {
            cur = match &cur {
                Type::Captured(next, c) => {
                    layers.push((true, self.zonk_caps(c)));
                    self.follow(next)
                }
                Type::Bounded(next, c) => {
                    layers.push((false, self.zonk_caps(c)));
                    self.follow(next)
                }
                _ => break,
            };
        }
        layers.into_iter().rev().fold(inner, |t, (hidden, c)| {
            if hidden {
                Self::qualify_hidden(t, &[c])
            } else {
                Type::Bounded(Rc::new(t), c)
            }
        })
    }

    /// Wraps `inner` with only the outer `Type::Bounded` value bounds of
    /// `outer`, skipping `Type::Captured` hidden qualifiers.
    pub(in crate::check) fn rewrap_bounds(&self, outer: &Type, inner: Type) -> Type {
        let mut layers = Vec::new();
        let mut cur = self.follow(outer);
        loop {
            cur = match &cur {
                Type::Captured(next, _) => self.follow(next),
                Type::Bounded(next, c) => {
                    layers.push(self.zonk_caps(c));
                    self.follow(next)
                }
                _ => break,
            };
        }
        layers
            .into_iter()
            .rev()
            .fold(inner, |t, c| Type::Bounded(Rc::new(t), c))
    }

    /// Unions the non-empty hidden qualifiers `qs` onto `t` without stacking
    /// duplicate `Type::Captured` layers.
    pub(in crate::check) fn qualify_hidden(t: Type, qs: &[Caps]) -> Type {
        let combined = qs.iter().fold(Caps::default(), |a, b| a.union(b));
        if combined.is_empty() {
            return t;
        }
        match t {
            Type::Captured(inner, c) => Type::Captured(inner, c.union(&combined)),
            other => Type::Captured(Rc::new(other), combined),
        }
    }

    /// The union of every qualifier of `t`, `∅` when bare.
    pub(in crate::check) fn qualifier(&self, t: &Type) -> Caps {
        self.qualifiers(t)
            .into_iter()
            .fold(Caps::default(), |out, c| out.union(&c))
    }

    /// Every qualifier of `t`, outermost first: a value of `(τ^{≤C})^{≤D}`,
    /// as `Ref.set` takes into a cell of `τ^{≤C}`, is within each.
    pub(in crate::check) fn qualifiers(&self, t: &Type) -> Vec<Caps> {
        let mut out = Vec::new();
        let mut t = self.follow(t);
        while let Type::Captured(inner, c) | Type::Bounded(inner, c) = &t {
            out.push(self.zonk_caps(c));
            t = self.follow(inner);
        }
        out
    }

    /// The hidden qualifiers of `t`, outermost first: what a function or a
    /// task of type `t` may use. A value bound `τ^{≤C}` says what the value
    /// reaches, not what using it needs: a closure that `Ref.set` stores in
    /// a cell of `Unit →R String` uses nothing.
    pub(in crate::check) fn hidden_qualifiers(&self, t: &Type) -> Vec<Caps> {
        let mut out = Vec::new();
        let mut t = self.follow(t);
        loop {
            t = match &t {
                Type::Captured(inner, c) => {
                    out.push(self.zonk_caps(c));
                    self.follow(inner)
                }
                Type::Bounded(inner, _) => self.follow(inner),
                _ => return out,
            };
        }
    }

    /// The innermost value bound of `t`: of `(τ^{≤C})^{≤D}` that `Ref.set`
    /// takes into a cell `Ref.make` made, the cell's capacity `C`.
    pub(in crate::check) fn innermost_bound(&self, t: &Type) -> Option<Caps> {
        let mut bound = None;
        let mut t = self.follow(t);
        loop {
            t = match &t {
                Type::Bounded(inner, c) => {
                    bound = Some(self.zonk_caps(c));
                    self.follow(inner)
                }
                Type::Captured(inner, _) => self.follow(inner),
                _ => return bound,
            };
        }
    }

    /// `t` once for each of its qualifiers, about the bare type: what is
    /// within `(τ^{≤C})^{≤D}` is within `τ^{≤C}` and within `τ^{≤D}`, not
    /// within what the two reach together.
    pub(in crate::check) fn layers(&self, t: &Type) -> Vec<Type> {
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
    /// hidden qualifiers: the body's stage `inner` within `stage` with its
    /// `Live`. What the body holds is no bound: a qualifier is what its use
    /// needs.
    pub(in crate::check) fn close_within(
        &mut self,
        ty: &Type,
        inner: &Stage,
        stage: &Stage,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let mut each = expected.map_or_else(Vec::new, |t| self.hidden_qualifiers(t));
        if each.is_empty() {
            each.push(Caps::default());
        }
        for q in &each {
            self.stage_within(inner, &live_stage(stage, q))?;
        }
        Ok(ty.clone())
    }

    /// Records `Outlives c r` to be proved under the facts currently in scope.
    pub(in crate::check) fn need_outlives(&mut self, c: Caps, r: Type) {
        let facts = self.regions.facts.clone();
        self.regions.needs.push((c, r, facts));
    }
}
