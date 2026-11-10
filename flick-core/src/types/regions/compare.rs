//! Capture-qualified type equality and value subsumption.

use std::mem;

use super::{Hold, Visible, capture_mismatch};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::{Norm, Type, capture_label};
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
                self.unify_qualifiers(&left, &right)
            }
        }
    }

    /// Unifies two capture qualifiers; inside a subsumption, ones that differ
    /// only by the captures of a type not inferred yet are compared when it is.
    pub fn unify_qualifiers(&mut self, left: &Type, right: &Type) -> Unified {
        let (left, right) = (self.expanded(left), self.expanded(right));
        match self.unify(&left, &right) {
            Err(mismatch) if self.unknown_captures(&mismatch) => {
                let span = self.regions.deferring.expect("checked");
                self.regions.deferred.push((left, right, span));
                Ok(())
            }
            other => other,
        }
    }

    /// Whether qualifiers differ only by the captures of a type not inferred
    /// yet, inside a subsumption that can compare them once it is.
    fn unknown_captures(&self, mismatch: &Mismatch) -> bool {
        let Mismatch::Label { label, .. } = mismatch else {
            return false;
        };
        self.regions.deferring.is_some() && self.uninferred(label)
    }

    /// Whether `label` is the captures `^?m` of a type not inferred yet.
    fn uninferred(&self, label: &str) -> bool {
        let meta = label.strip_prefix("^?").and_then(|m| m.parse().ok());
        meta.is_some_and(|m| {
            !self.store.is_capture_row(m)
                && matches!(self.store.resolve(&Type::Meta(m)), Type::Meta(_))
        })
    }

    /// A capture row whose entries are what they hold now: the entry of a
    /// type variable since solved to a type is that type's captures.
    pub fn expanded(&self, row: &Type) -> Type {
        let norm = self.store.normalize(row);
        let atom = |t: &Type| capture_label(&self.store.resolve(t)).is_some();
        if norm
            .entries
            .iter()
            .all(|(label, t)| !label.starts_with('^') || atom(t))
        {
            return row.clone();
        }
        let mut out = Norm::default();
        for (label, t) in norm.entries {
            if !label.starts_with('^') || atom(&t) {
                out.entries.push((label, t));
                continue;
            }
            match self.store.resolve(&t) {
                row @ (Type::Row(_) | Type::Union(..)) => {
                    self.capture_row_filtered(&row, &mut out, false);
                }
                other => self.collect_captures(&other, &mut out),
            }
        }
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
        out.tails.extend(norm.tails);
        out.into_type()
    }

    /// A deferred qualifier at the end of a declaration: a type it never
    /// inferred holds nothing, as no value of it was made (`None`, `[]`).
    fn inferred(&self, row: &Type) -> Type {
        let mut norm = self.store.normalize(&self.expanded(row));
        norm.entries.retain(|(label, _)| !self.uninferred(label));
        norm.into_type()
    }

    /// Whether a region diagnostic already covers `span`.
    fn hold_reported(&self, span: Span) -> bool {
        self.diagnostics.iter().any(|d| {
            [codes::G001, codes::G002].contains(&d.code)
                && d.span
                    .is_some_and(|s| s.start <= span.start && span.end <= s.end)
        })
    }

    /// Compares the qualifiers deferred until their types were inferred, and
    /// reports the captures no hold named.
    pub fn compare_deferred(&mut self) {
        for (left, right, span) in mem::take(&mut self.regions.deferred) {
            let (left, right) = (self.inferred(&left), self.inferred(&right));
            if let Err(mismatch) = self.unify(&left, &right)
                && !self.hold_reported(span)
            {
                self.mismatch(&mismatch, &left, &right, span);
            }
        }
        for (mismatch, actual, expected, span) in mem::take(&mut self.regions.held) {
            if !self.hold_reported(span) {
                self.mismatch(&mismatch, &actual, &expected, span);
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
        let (a, e) = self.live_sides((a, ca.as_ref()), (e, ce.as_ref()));
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
        self.held_fits(fits, mark, actual, expected, span)
    }

    /// The outcome of subsuming a value its holds since `mark` are about.
    pub fn held_fits(
        &mut self,
        fits: Unified,
        mark: usize,
        actual: &Type,
        expected: &Type,
        span: Span,
    ) -> Unified {
        match fits {
            // what a value holds beyond its position is the hold's to name
            Err(mismatch) if capture_mismatch(&mismatch) => {
                let held = (mismatch, actual.clone(), expected.clone(), span);
                self.regions.held.push(held);
                Ok(())
            }
            // a value whose type does not fit is reported once, by the type
            Err(mismatch) => {
                self.regions.holds.truncate(mark);
                Err(mismatch)
            }
            Ok(()) => Ok(()),
        }
    }
}
