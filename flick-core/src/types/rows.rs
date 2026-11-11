//! Unification of rows: record fields and the
//! service and error rows of runtime arrows. Labels are unique and their order
//! does not matter; a row without a tail is closed, so a record with extra
//! fields never passes for a smaller one (no width subtyping). A row may
//! extend several row variables, `r1 + r2`; equal ones cancel out.

use std::cmp::Ordering;

use super::Checker;
use super::elem::Elem;
use super::ty::{Norm, Row, Type, Written};
use super::unify::{Mismatch, Unified};

impl Checker<'_> {
    pub fn unify_rows(&mut self, left: &Row, right: &Row) -> Unified {
        let (left, right) = (Type::Row(left.clone()), Type::Row(right.clone()));
        self.unify_row_types(&left, &right)
    }

    /// Unifies two types of a row kind: rows, unions and row variables.
    #[allow(clippy::too_many_lines)]
    pub fn unify_row_types(&mut self, left: &Type, right: &Type) -> Unified {
        // what a variable is solved to keeps where the row it comes from is written
        let (left_written, right_written) = (
            self.store.resolve(left).written(),
            self.store.resolve(right).written(),
        );
        let mut left_norm = self.store.normalize(left);
        let mut right_norm = self.store.normalize(right);
        if left_norm
            .entries
            .iter()
            .chain(&right_norm.entries)
            .any(|(label, _)| matches!(label, Elem::Capture(_)))
        {
            left_norm = Norm::default();
            right_norm = Norm::default();
            self.capture_row(left, &mut left_norm);
            self.capture_row(right, &mut right_norm);
        }
        self.merge_same_names(&mut left_norm)?;
        self.merge_same_names(&mut right_norm)?;
        self.unify_live(&mut left_norm, &mut right_norm);
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
        let missing = |only: &[(Elem, Type)], left| {
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
                self.bind(
                    *m,
                    &written(extended(only_right, tails_right), right_written),
                )
            }
            (_, [Type::Meta(m)]) => {
                missing(&only_right, false)?;
                self.bind(*m, &written(extended(only_left, tails_left), left_written))
            }
            // `?m` less some names against a row: the least `?m` is that row,
            // which then holds none of the names
            ([Type::Without { row, gone }], _) if matches!(**row, Type::Meta(_)) => {
                missing(&only_left, true)?;
                let Type::Meta(m) = **row else {
                    unreachable!("matched above")
                };
                self.bind(m, &left_out(only_right, tails_right, gone, false)?)
            }
            (_, [Type::Without { row, gone }]) if matches!(**row, Type::Meta(_)) => {
                missing(&only_right, false)?;
                let Type::Meta(m) = **row else {
                    unreachable!("matched above")
                };
                self.bind(m, &left_out(only_left, tails_left, gone, true)?)
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
        only_left: &mut Vec<(Elem, Type)>,
        only_right: &mut Vec<(Elem, Type)>,
        tails_left: &[Type],
        tails_right: &[Type],
    ) -> Unified {
        let variable = |only: &[(Elem, Type)]| matches!(only, [(Elem::Error(_), _)]);
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
                // what each `Live` needs alive, `regions/live.rs`, is needed together
                if norm.entries[i - 1].0 == Elem::Live {
                    norm.entries[i - 1].1 = Type::Union(Box::new(kept), Box::new(same));
                    continue;
                }
                self.unify(&kept, &same)?;
            } else {
                i += 1;
            }
        }
        Ok(())
    }
}

/// The row a variable less the names `gone` equals: it may hold none of them,
/// or no row less them is this one. `left` says which side holds the entries.
fn left_out(
    entries: Vec<(Elem, Type)>,
    tails: Vec<Type>,
    gone: &[Elem],
    left: bool,
) -> Result<Type, Mismatch> {
    match entries.iter().find(|(label, _)| gone.contains(label)) {
        Some((label, _)) => Err(Mismatch::Label {
            label: label.clone(),
            left,
        }),
        None => Ok(extended(entries, tails)),
    }
}

fn extended(entries: Vec<(Elem, Type)>, tails: Vec<Type>) -> Type {
    Norm { entries, tails }.into_type()
}

/// `row` with where it is written, when that is known.
fn written(mut row: Type, place: Written) -> Type {
    if let Type::Row(r) = &mut row
        && place.0.is_some()
    {
        r.written = place;
    }
    row
}
