//! Unification of rows: record fields and the
//! service and error rows of runtime arrows. Labels are unique and their order
//! does not matter; a row without a tail is closed, so a record with extra
//! fields never passes for a smaller one (no width subtyping). A row may
//! extend several row variables, `r1 + r2`; equal ones cancel out.

use std::cmp::Ordering;

use super::Checker;
use super::ty::{Norm, Row, Type};
use super::unify::{Mismatch, Unified};

impl Checker<'_> {
    pub fn unify_rows(&mut self, left: &Row, right: &Row) -> Unified {
        let (left, right) = (Type::Row(left.clone()), Type::Row(right.clone()));
        self.unify_row_types(&left, &right)
    }

    /// Unifies two types of a row kind: rows, unions and row variables.
    #[allow(clippy::too_many_lines)]
    pub fn unify_row_types(&mut self, left: &Type, right: &Type) -> Unified {
        let mut left_norm = self.store.normalize(left);
        let mut right_norm = self.store.normalize(right);
        if left_norm
            .entries
            .iter()
            .chain(&right_norm.entries)
            .any(|(label, _)| label.starts_with('^'))
        {
            left_norm = Norm::default();
            right_norm = Norm::default();
            self.capture_row(left, &mut left_norm);
            self.capture_row(right, &mut right_norm);
        }
        self.merge_same_names(&mut left_norm)?;
        self.merge_same_names(&mut right_norm)?;
        let (left, right) = (left_norm, right_norm);
        let (mut only_left, mut only_right) = (Vec::new(), Vec::new());
        let (mut i, mut j) = (0, 0);
        while i < left.entries.len() || j < right.entries.len() {
            let order = match (left.entries.get(i), right.entries.get(j)) {
                (Some(left_field), Some(right_field)) => left_field.0.cmp(&right_field.0),
                (Some(_), None) => Ordering::Less,
                (None, _) => Ordering::Greater,
            };
            match order {
                Ordering::Equal => {
                    self.unify(&left.entries[i].1, &right.entries[j].1)?;
                    i += 1;
                    j += 1;
                }
                Ordering::Less => {
                    only_left.push(left.entries[i].clone());
                    i += 1;
                }
                Ordering::Greater => {
                    only_right.push(right.entries[j].clone());
                    j += 1;
                }
            }
        }
        let tails_left: Vec<Type> = left
            .tails
            .iter()
            .filter(|t| !right.tails.contains(t))
            .cloned()
            .collect();
        let tails_right: Vec<Type> = right
            .tails
            .iter()
            .filter(|t| !left.tails.contains(t))
            .cloned()
            .collect();
        if tails_left.contains(&Type::Error) || tails_right.contains(&Type::Error) {
            return Ok(());
        }
        self.pair_variable(&mut only_left, &mut only_right, &tails_left, &tails_right)?;
        let missing = |only: &[(String, Type)], left| {
            only.first().map_or(Ok(()), |(label, _)| {
                Err(Mismatch::Label {
                    label: label.clone(),
                    left,
                })
            })
        };
        match (&tails_left[..], &tails_right[..]) {
            ([], []) => {
                missing(&only_left, true)?;
                missing(&only_right, false)
            }
            ([Type::Meta(a)], [Type::Meta(b)]) => {
                let rest = self.fresh();
                // the shared rest is a row of the kind of the rows it ends
                if let Type::Meta(r) = rest {
                    self.store.mark_like(r, *a);
                    self.store.mark_like(r, *b);
                }
                self.bind(*a, &extended(only_right, vec![rest.clone()]))?;
                self.bind(*b, &extended(only_left, vec![rest]))
            }
            ([Type::Meta(m)], _) => {
                missing(&only_left, true)?;
                self.bind(*m, &extended(only_right, tails_right))
            }
            (_, [Type::Meta(m)]) => {
                missing(&only_right, false)?;
                self.bind(*m, &extended(only_left, tails_left))
            }
            // `r1 + r2` against what is left empty: each variable is empty
            (metas, []) | ([], metas)
                if only_left.is_empty()
                    && only_right.is_empty()
                    && metas.iter().all(|t| matches!(t, Type::Meta(_))) =>
            {
                metas
                    .iter()
                    .try_for_each(|t| self.unify(t, &Type::empty_row()))
            }
            ([a], [b]) => {
                missing(&only_left, true)?;
                missing(&only_right, false)?;
                let (a, b) = (a.clone(), b.clone());
                self.unify(&a, &b)
            }
            _ => {
                missing(&only_left, true)?;
                missing(&only_right, false)?;
                Err(Mismatch::Types(left.into_type(), right.into_type()))
            }
        }
    }
}

impl Checker<'_> {
    /// A closed row whose one unmatched element is a type variable, `{t}`
    /// against `{NotFound}`: the variable is that element.
    fn pair_variable(
        &mut self,
        only_left: &mut Vec<(String, Type)>,
        only_right: &mut Vec<(String, Type)>,
        tails_left: &[Type],
        tails_right: &[Type],
    ) -> Unified {
        let variable =
            |only: &[(String, Type)]| matches!(only, [(label, _)] if label.starts_with('~'));
        let pair = match (&only_left[..], &only_right[..]) {
            ([(_, l)], [(_, r)])
                if variable(only_left) && tails_left.is_empty()
                    || variable(only_right) && tails_right.is_empty() =>
            {
                Some((l.clone(), r.clone()))
            }
            _ => None,
        };
        if let Some((l, r)) = pair {
            self.unify(&l, &r)?;
            only_left.clear();
            only_right.clear();
        }
        Ok(())
    }

    /// Unifies the elements of a row that have one name: `ValidationError a`
    /// and `ValidationError b` once a variable became the first.
    fn merge_same_names(&mut self, norm: &mut Norm) -> Unified {
        let mut i = 1;
        while i < norm.entries.len() {
            if norm.entries[i].0 == norm.entries[i - 1].0 {
                let (kept, same) = (norm.entries[i - 1].1.clone(), norm.entries.remove(i).1);
                self.unify(&kept, &same)?;
            } else {
                i += 1;
            }
        }
        Ok(())
    }
}

fn extended(entries: Vec<(String, Type)>, tails: Vec<Type>) -> Type {
    Norm { entries, tails }.into_type()
}
