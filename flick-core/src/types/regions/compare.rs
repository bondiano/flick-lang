//! Capture-qualified type equality and value subsumption.

use std::mem;

use super::{Hold, Visible, capture_mismatch};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::Type;
use crate::types::unify::{Mismatch, Unified};

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
                match self.unify(&left, &right) {
                    Err(mismatch) if self.unknown_captures(&mismatch) => {
                        let span = self.regions.deferring.expect("checked");
                        self.regions.deferred.push((left, right, span));
                        Ok(())
                    }
                    other => other,
                }
            }
        }
    }

    /// Whether qualifiers differ only by the captures of a type not inferred
    /// yet, inside a subsumption that can compare them once it is.
    fn unknown_captures(&self, mismatch: &Mismatch) -> bool {
        let Mismatch::Label { label, .. } = mismatch else {
            return false;
        };
        let meta = label.strip_prefix("^?").and_then(|m| m.parse().ok());
        self.regions.deferring.is_some()
            && meta.is_some_and(|m| {
                !self.store.is_capture_row(m)
                    && matches!(self.store.resolve(&Type::Meta(m)), Type::Meta(_))
            })
    }

    /// Compares the qualifiers deferred until their types were inferred.
    pub fn compare_deferred(&mut self) {
        for (left, right, span) in mem::take(&mut self.regions.deferred) {
            let reported = self.diagnostics.iter().any(|d| {
                [codes::G001, codes::G002].contains(&d.code)
                    && d.span
                        .is_some_and(|s| s.start <= span.start && span.end <= s.end)
            });
            if let Err(mismatch) = self.unify(&left, &right)
                && !reported
            {
                self.mismatch(&mismatch, &left, &right, span);
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
