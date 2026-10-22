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
    pub fn unify_row_types(&mut self, left: &Type, right: &Type) -> Unified {
        let left = self.store.normalize(left);
        let right = self.store.normalize(right);
        let (mut only_left, mut only_right) = (Vec::new(), Vec::new());
        let (mut i, mut j) = (0, 0);
        while i < left.entries.len() || j < right.entries.len() {
            let order = match (left.entries.get(i), right.entries.get(j)) {
                (Some(l), Some(r)) => l.0.cmp(&r.0),
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

fn extended(entries: Vec<(String, Type)>, tails: Vec<Type>) -> Type {
    Norm { entries, tails }.into_type()
}
