//! Unification of rows: record fields (§5 "Структурность records") and the
//! service and error rows of runtime arrows. Labels are unique and their order
//! does not matter; a row without a tail is closed, so a record with extra
//! fields never passes for a smaller one (no width subtyping).

use super::Checker;
use super::ty::{Row, Type};
use super::unify::{Mismatch, Unified};

impl Checker<'_> {
    pub fn unify_rows(&mut self, left: &Row, right: &Row) -> Unified {
        let left = self.flatten(left);
        let right = self.flatten(right);
        let (mut only_left, mut only_right) = (Vec::new(), Vec::new());
        let (mut i, mut j) = (0, 0);
        while i < left.entries.len() || j < right.entries.len() {
            let order = match (left.entries.get(i), right.entries.get(j)) {
                (Some(l), Some(r)) => l.0.cmp(&r.0),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, _) => std::cmp::Ordering::Greater,
            };
            match order {
                std::cmp::Ordering::Equal => {
                    self.unify(&left.entries[i].1, &right.entries[j].1)?;
                    i += 1;
                    j += 1;
                }
                std::cmp::Ordering::Less => {
                    only_left.push(left.entries[i].clone());
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    only_right.push(right.entries[j].clone());
                    j += 1;
                }
            }
        }
        let missing = |only: &[(String, Type)], left| {
            only.first().map_or(Ok(()), |(label, _)| {
                Err(Mismatch::Label {
                    label: label.clone(),
                    left,
                })
            })
        };
        let tails = (left.tail.as_deref(), right.tail.as_deref());
        match tails {
            (Some(Type::Error), _) | (_, Some(Type::Error)) => Ok(()),
            (None, None) => {
                missing(&only_left, true)?;
                missing(&only_right, false)
            }
            (Some(Type::Meta(a)), Some(Type::Meta(b))) if a == b => {
                missing(&only_left, true)?;
                missing(&only_right, false)
            }
            (Some(Type::Meta(a)), Some(Type::Meta(b))) => {
                let rest = self.fresh();
                self.bind(*a, &extended(only_right, Some(rest.clone())))?;
                self.bind(*b, &extended(only_left, Some(rest)))
            }
            (Some(Type::Meta(m)), other) => {
                missing(&only_left, true)?;
                self.bind(*m, &extended(only_right, other.cloned()))
            }
            (other, Some(Type::Meta(m))) => {
                missing(&only_right, false)?;
                self.bind(*m, &extended(only_left, other.cloned()))
            }
            (Some(a), Some(b)) => {
                missing(&only_left, true)?;
                missing(&only_right, false)?;
                let (a, b) = (a.clone(), b.clone());
                self.unify(&a, &b)
            }
            (Some(_), None) | (None, Some(_)) => {
                missing(&only_left, true)?;
                missing(&only_right, false)?;
                let (a, b) = (Type::Row(left), Type::Row(right));
                Err(Mismatch::Types(a, b))
            }
        }
    }

    /// The row with solved tails merged in and its final tail resolved.
    fn flatten(&self, row: &Row) -> Row {
        let mut entries = row.entries.clone();
        let mut tail = row.tail.as_ref().map(|t| self.store.resolve(t));
        while let Some(Type::Row(inner)) = tail {
            entries.extend(inner.entries);
            tail = inner.tail.map(|t| self.store.resolve(&t));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Row {
            entries,
            tail: tail.map(Box::new),
        }
    }
}

fn extended(entries: Vec<(String, Type)>, tail: Option<Type>) -> Type {
    match tail {
        Some(tail) if entries.is_empty() => tail,
        tail => Type::Row(Row {
            entries,
            tail: tail.map(Box::new),
        }),
    }
}
