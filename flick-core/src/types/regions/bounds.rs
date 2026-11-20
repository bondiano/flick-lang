//! Subsumption of whole-value `Type::Bounded` capture bounds.

use super::{Hold, Visible};
use crate::source::Span;
use crate::types::Checker;
use crate::types::gadt::row_facts::RowKind;
use crate::types::kinds::Kind;
use crate::types::ty::{Norm, Type};
use crate::types::unify::Unified;

impl Checker<'_> {
    /// A bound refines a complete generic type; unlike a nominal qualifier,
    /// it cannot replace that type's own hidden captures on instantiation.
    pub fn subsume_bound(&mut self, actual: &Type, expected: &Type, span: Span) -> Option<Unified> {
        let mark = self.cx.regions.holds.len();
        let fits = self.subsume_bound_at(actual, expected, span)?;
        Some(self.held_fits(fits, mark, actual, expected, span))
    }

    fn inferred_bound_capture(&self, allowed: &Norm, captures: &Type) -> bool {
        let held = self.store.normalize(captures);
        let open = |ty: &Type| matches!(self.store.resolve(ty), Type::Meta(_));
        !allowed.tails.is_empty()
            && allowed.tails.iter().all(open)
            && held.entries.iter().all(|(_, ty)| open(ty))
            && held.tails.iter().all(open)
    }

    fn subsume_bound_at(&mut self, actual: &Type, expected: &Type, span: Span) -> Option<Unified> {
        let a = self.bound_head(actual);
        let e = self.bound_head(expected);
        if matches!(e, Type::Bounded { .. }) {
            return Some(self.subsume_bounded_expected(actual, expected, &a, e, span));
        }
        // a position not known yet takes a value bounded by a region with its
        // bound: what it turns out to be may store the region's resource
        if let Type::Bounded { ty, captures } = &a
            && let Some(m) = e.inference_meta_root()
            && !self.store.is_capture_row(m)
            && !self.store.is_numeric(m)
            && self.bounds_a_region(captures)
            && (self.hidden_fields(ty, &mut Vec::new()) || !self.captures_of(ty).entries.is_empty())
        {
            return Some(
                self.bind_subsumed_meta(actual, expected, false)
                    .unwrap_or_else(|| self.unify(expected, actual)),
            );
        }
        if let Type::Bounded { ty, captures } = &a
            && !matches!(e, Type::Captured { .. })
        {
            let identity = self.capture_identity(ty);
            // what a function, task or lazy hides it may hold: its use is in its
            // row (`with_live_bound` carries live regions/capture rows onto runtime stages)
            let callable = self.is_callable(expected);
            if self.hidden_fields(expected, &mut Vec::new())
                && (!callable || self.holds_reader(&[(None, actual.clone())]))
            {
                // the hidden row of data is no region another argument shows
                let visible = if self.stores_hidden(expected) {
                    Visible::Now(Vec::new())
                } else {
                    Visible::Refined(expected.clone())
                };
                self.hold(Hold {
                    held: vec![(None, actual.clone())],
                    visible,
                    bound: Type::empty_row(),
                    expected: Some(expected.clone()),
                    written: true,
                    what: "value",
                    span,
                });
            }
            let identity = if callable {
                self.with_live_bound(identity, captures)
            } else {
                identity
            };
            let callable = self.bounded_callable(identity, captures);
            return Some(self.subsume(&callable, expected, span));
        }
        None
    }

    fn subsume_bounded_expected(
        &mut self,
        actual: &Type,
        expected: &Type,
        a: &Type,
        mut base: Type,
        span: Span,
    ) -> Unified {
        let mut allowed = Norm::default();
        let mut bound_depth = 0usize;
        while let Type::Bounded { ty, captures } = base {
            bound_depth += 1;
            self.capture_row(&captures, &mut allowed);
            self.hold(Hold {
                held: vec![(None, actual.clone())],
                visible: self.bounded_visible(&ty),
                bound: *captures.clone(),
                expected: Some(expected.clone()),
                written: !matches!(self.store.resolve(&captures), Type::Meta(_)),
                what: "value",
                span,
            });
            base = self.bound_head(&ty);
        }
        // a value qualifier the bound already allows says nothing more:
        // the variable takes the data without it; a closure's stored
        // qualifier stays part of its type. Under a value bound, which
        // alone gives what the value reaches (§2.1), a qualifier not
        // inferred yet says nothing either when the bound expected is
        // not inferred yet: the holds above grow it by the value (`docs/formal/regions.md#a-cell-created-empty-accepts-a-resource-of-its-scope`)
        let mut inner = self.qualified_head(actual);
        while let Type::Bounded { ty, .. } = inner {
            inner = self.qualified_head(&ty);
        }
        let identity = match (self.qualified_head(actual), inner) {
            (
                Type::Captured {
                    ty,
                    captures,
                    value: true,
                    ..
                },
                _,
            ) if !self.is_callable(&ty)
                && !self.stores_hidden(&ty)
                && self.within(&captures, &allowed) =>
            {
                self.capture_identity(&ty)
            }
            (
                _,
                Type::Captured {
                    ty,
                    captures,
                    value: true,
                    ..
                },
            ) if !self.is_callable(&ty)
                && !self.stores_hidden(&ty)
                && matches!(self.qualified_head(actual), Type::Bounded { .. })
                && self.inferred_bound_capture(&allowed, &captures) =>
            {
                self.capture_identity(&ty)
            }
            _ => self.capture_identity(a),
        };
        if bound_depth == 1
            && !matches!(base, Type::Meta(_))
            && !self.is_callable(&identity)
            && self.stores_hidden(&identity)
            && let Some(captures) = self.bound_captures(a)
            && self.bounds_a_region(&captures)
        {
            return self.subsume(a, &base, span);
        }
        let identity = match self.bound_captures(a) {
            Some(captures) if self.is_callable(&identity) => {
                self.with_live_bound(identity, &captures)
            }
            _ => identity,
        };
        let fits = self.callable_bound_fits(&identity, &base, span);
        fits.unwrap_or_else(|| self.unify(&identity, &base))
    }

    /// Whether a value of `t` can hold nothing, so a qualifier on it says
    /// nothing: a scalar, or data without hidden storage whose type shows
    /// no captures.
    pub(super) fn holds_nothing(&self, t: &Type) -> bool {
        if self.scalar(t) {
            return true;
        }
        if matches!(
            *self.borrowed_whnf(t),
            Type::Var(_) | Type::Meta(_) | Type::Skolem(_)
        ) {
            return false;
        }
        let captures = self.captures_of(t);
        captures.entries.is_empty()
            && captures.tails.is_empty()
            && !self.hidden_fields(t, &mut Vec::new())
    }

    /// Whether every capture of the row `captures` is one `allowed` names.
    fn within(&self, captures: &Type, allowed: &Norm) -> bool {
        if !self.store.row_equalities.is_empty()
            && self.row_implied(captures, &allowed.clone().into_type(), RowKind::Captures)
        {
            return true;
        }
        let mut held = Norm::default();
        self.capture_row(captures, &mut held);
        held.tails.is_empty()
            && held
                .entries
                .iter()
                .all(|(label, _)| allowed.get(label).is_some())
    }

    /// Whether a bound names a region, not only variables of generic code.
    fn bounds_a_region(&self, captures: &Type) -> bool {
        let mut norm = Norm::default();
        self.capture_row(captures, &mut norm);
        norm.entries
            .iter()
            .any(|(_, atom)| matches!(self.atom_kind(atom, &[]), Some(Kind::Region)))
    }
}
