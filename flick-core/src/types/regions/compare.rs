//! Capture-qualified type equality and value subsumption.

use super::{Hold, Visible, capture_mismatch};
use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::Type;
use crate::types::unify::Unified;

impl Checker<'_> {
    /// Unifies types of which at least one has a capture qualifier: the
    /// qualifiers are equal, as in mutable storage.
    pub fn unify_captured(
        &mut self,
        (a, ca): (Type, Option<Type>),
        (b, cb): (Type, Option<Type>),
    ) -> Unified {
        let captured = |t: &Type, c: &Option<Type>| match c {
            Some(c) => Type::Captured {
                ty: Box::new(t.clone()),
                captures: Box::new(c.clone()),
                value: false,
            },
            None => t.clone(),
        };
        match (&a, &b) {
            (Type::Error, _) | (_, Type::Error) => Ok(()),
            (Type::Meta(m), _) if ca.is_none() => self.bind(*m, &captured(&b, &cb)),
            (_, Type::Meta(m)) if cb.is_none() => self.bind(*m, &captured(&a, &ca)),
            _ => {
                // A bare type adds no hidden captures; its own fields and rows
                // are compared by the types themselves below.
                let left = ca.unwrap_or_else(Type::empty_row);
                let right = cb.unwrap_or_else(Type::empty_row);
                // the types first: what they solve decides what a qualifier holds
                self.unify(&a, &b)?;
                self.unify(&left, &right)
            }
        }
    }

    /// Subsumption where a qualifier is involved: a value fits a position
    /// whose qualifier covers what it holds; a bare position takes no
    /// capture its type does not show.
    pub fn subsume_captured(
        &mut self,
        (actual, (a, ca)): (&Type, (Type, Option<Type>)),
        (expected, (e, ce)): (&Type, (Type, Option<Type>)),
        span: Span,
    ) -> Unified {
        match (&a, &e) {
            (Type::Error, _) | (_, Type::Error) => return Ok(()),
            (_, Type::Meta(_)) if ce.is_none() => return self.unify(actual, expected),
            (Type::Meta(_), _) if ca.is_none() => return self.unify(actual, expected),
            _ => {}
        }
        let visible = if ce.is_some() {
            Visible::Now(Vec::new())
        } else {
            Visible::Late(e.clone())
        };
        let bound = ce.unwrap_or_else(Type::empty_row);
        let written = !matches!(self.store.resolve(&bound), Type::Meta(_));
        let mark = self.regions.holds.len();
        self.hold(Hold {
            held: vec![(None, actual.clone())],
            visible,
            bound,
            expected: Some(expected.clone()),
            written,
            what: "value",
            span,
        });
        let a = match ca {
            Some(_) => self.bounded_callable(a, &self.captures_of(actual).into_type()),
            None => a,
        };
        let fits = self.subsume(&a, &e, span);
        // a value whose type does not fit is reported once, by the type
        if fits.as_ref().is_err_and(|m| !capture_mismatch(m)) {
            self.regions.holds.truncate(mark);
        }
        fits
    }
}
