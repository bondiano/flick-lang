//! An independent decision procedure for branch-local row equations.
//! Enumerate Boolean memberships of each named label and one fresh label.
//! This checks every residual, rather than selecting a unification witness.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::{mem, slice};

use super::{Check, Checker, Solved, error};
use crate::syntax::{Name, Path};
use crate::types::{Caps, Row, Type, Var};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default)]
pub(super) struct Givens {
    pub types: Vec<(Name, Solved)>,
    pub rows: Vec<(Row, Row)>,
    pub caps: Vec<(Caps, Caps)>,
}

impl Givens {
    fn is_empty(&self) -> bool {
        self.types.is_empty() && self.rows.is_empty() && self.caps.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Tail(Var),
    Atom(Var),
    Entry(usize),
}

#[derive(Default)]
struct Membership {
    known: bool,
    vars: BTreeSet<Symbol>,
}

impl Membership {
    fn value(&self, assignment: &BTreeMap<Symbol, bool>) -> Option<bool> {
        if self.known || self.vars.iter().any(|v| assignment.get(v) == Some(&true)) {
            Some(true)
        } else if self.vars.iter().all(|v| assignment.contains_key(v)) {
            Some(false)
        } else {
            None
        }
    }
}

fn at(
    row: &Row,
    label: Option<&Path>,
    payload: Option<&Rc<[Type]>>,
    entries: &[Rc<[Type]>],
) -> Membership {
    let mut result = Membership {
        known: false,
        vars: row
            .vars
            .iter()
            .filter(|v| !label.is_some_and(|l| row.less.contains(&((*v).clone(), l.clone()))))
            .cloned()
            .map(Symbol::Tail)
            .collect(),
    };
    if let Some(args) = label.and_then(|l| row.elems.get(l))
        && let Some(expected) = payload
    {
        if args == expected {
            result.known = true;
        } else {
            // An open payload may later equal this instance. All occurrences
            // of the same explicit instance share its membership variable.
            let id = entries
                .iter()
                .position(|a| a == args)
                .expect("enumerated instance");
            result.vars.insert(Symbol::Entry(id));
        }
    }
    result
}

fn captures(c: &Caps) -> Membership {
    Membership {
        known: false,
        // A type atom may reach no captures, or overlap another atom. Its
        // membership is independent; region identities are never equated.
        vars: c
            .atoms
            .iter()
            .cloned()
            .map(Symbol::Atom)
            .chain(c.vars.iter().cloned().map(Symbol::Tail))
            .collect(),
    }
}

/// Is there an assignment satisfying all equations and, when supplied,
/// violating the proposed inclusion? No fixed variable/count limit.
fn witness(
    equations: &[(Membership, Membership)],
    query: Option<(&Membership, &Membership)>,
) -> bool {
    fn search(
        equations: &[(Membership, Membership)],
        query: Option<(&Membership, &Membership)>,
        symbols: &[Symbol],
        assignment: &mut BTreeMap<Symbol, bool>,
    ) -> bool {
        if equations.iter().any(|(a, b)| {
            matches!((a.value(assignment), b.value(assignment)),
            (Some(x), Some(y)) if x != y)
        }) {
            return false;
        }
        if query.is_some_and(|(a, b)| {
            a.value(assignment) == Some(false) || b.value(assignment) == Some(true)
        }) {
            return false;
        }
        let Some((symbol, rest)) = symbols.split_first() else {
            return true;
        };
        for value in [false, true] {
            assignment.insert(symbol.clone(), value);
            if search(equations, query, rest, assignment) {
                return true;
            }
        }
        assignment.remove(symbol);
        false
    }
    let mut symbols = BTreeSet::new();
    for (a, b) in equations.iter().map(|(a, b)| (a, b)).chain(query) {
        symbols.extend(a.vars.iter().chain(&b.vars).cloned());
    }
    search(
        equations,
        query,
        &symbols.into_iter().collect::<Vec<_>>(),
        &mut BTreeMap::new(),
    )
}

impl Checker<'_> {
    /// Constructor candidates for exhaustiveness must satisfy all facts
    /// learned in earlier columns, rather than just their own equations.
    pub(super) fn coverage_consistent(&self) -> bool {
        let pairs: Vec<_> = self
            .row_equalities
            .iter()
            .map(|(a, b)| (self.zonk_row(a), self.zonk_row(b)))
            .collect();
        let mut labels = BTreeSet::from([None]);
        for row in pairs.iter().flat_map(|(a, b)| [a, b]) {
            labels.extend(row.elems.keys().cloned().map(Some));
            labels.extend(row.less.iter().map(|(_, label)| Some(label.clone())));
        }
        let rows_consistent = labels.into_iter().all(|label| {
            let mut entries = Vec::new();
            for row in pairs.iter().flat_map(|(a, b)| [a, b]) {
                if let Some(args) = label.as_ref().and_then(|label| row.elems.get(label))
                    && !entries.contains(args)
                {
                    entries.push(args.clone());
                }
            }
            entries.iter().map(Some).chain([None]).all(|payload| {
                let equations: Vec<_> = pairs
                    .iter()
                    .map(|(a, b)| {
                        (
                            at(a, label.as_ref(), payload, &entries),
                            at(b, label.as_ref(), payload, &entries),
                        )
                    })
                    .collect();
                witness(&equations, None)
            })
        });
        let caps: Vec<_> = self
            .cap_equalities
            .iter()
            .map(|(a, b)| (captures(&self.zonk_caps(a)), captures(&self.zonk_caps(b))))
            .collect();
        rows_consistent && witness(&caps, None)
    }

    pub(super) fn branch_givens(&self) -> Givens {
        Givens {
            types: self.equalities.clone(),
            rows: self.row_equalities.clone(),
            caps: self.cap_equalities.clone(),
        }
    }

    pub(super) fn under_givens<T>(&mut self, facts: &Givens, f: impl FnOnce(&mut Self) -> T) -> T {
        if facts.is_empty()
            && self.equalities.is_empty()
            && self.row_equalities.is_empty()
            && self.cap_equalities.is_empty()
        {
            return f(self);
        }
        let types = mem::replace(&mut self.equalities, facts.types.clone());
        let rows = mem::replace(&mut self.row_equalities, facts.rows.clone());
        let caps = mem::replace(&mut self.cap_equalities, facts.caps.clone());
        let result = f(self);
        self.equalities = types;
        self.row_equalities = rows;
        self.cap_equalities = caps;
        result
    }

    pub(super) fn row_implied(&self, a: &Row, b: &Row) -> bool {
        if self.row_equalities.is_empty() {
            return false;
        }
        let pairs: Vec<_> = self
            .row_equalities
            .iter()
            .map(|(a, b)| (self.zonk_row(a), self.zonk_row(b)))
            .collect();
        let (a, b) = (self.zonk_row(a), self.zonk_row(b));
        let rows: Vec<_> = pairs
            .iter()
            .flat_map(|(a, b)| [a, b])
            .chain([&a, &b])
            .collect();
        let mut labels = BTreeSet::from([None]);
        for row in &rows {
            labels.extend(row.elems.keys().cloned().map(Some));
            labels.extend(row.less.iter().map(|(_, n)| Some(n.clone())));
        }
        labels.into_iter().all(|label| {
            let mut entries = Vec::new();
            for row in &rows {
                if let Some(args) = label.as_ref().and_then(|l| row.elems.get(l))
                    && !entries.contains(args)
                {
                    entries.push(args.clone());
                }
            }
            entries.iter().map(Some).chain([None]).all(|payload| {
                let equations: Vec<_> = pairs
                    .iter()
                    .map(|(a, b)| {
                        (
                            at(a, label.as_ref(), payload, &entries),
                            at(b, label.as_ref(), payload, &entries),
                        )
                    })
                    .collect();
                !witness(
                    &equations,
                    Some((
                        &at(&a, label.as_ref(), payload, &entries),
                        &at(&b, label.as_ref(), payload, &entries),
                    )),
                )
            })
        })
    }

    pub(super) fn caps_implied(&self, a: &Caps, b: &Caps) -> bool {
        let equations: Vec<_> = self
            .cap_equalities
            .iter()
            .map(|(a, b)| (captures(&self.zonk_caps(a)), captures(&self.zonk_caps(b))))
            .collect();
        !witness(
            &equations,
            Some((&captures(&self.zonk_caps(a)), &captures(&self.zonk_caps(b)))),
        )
    }

    /// A constructed scrutinee may still have inference metas in its
    /// index. Choose those by an ordinary inclusion, then require that the
    /// branch equation proves the query for every hidden residual. Pattern
    /// existentials have already become rigid; none is solved here.
    pub(super) fn infer_row_upper(&mut self, actual: &Row, allowed: &Row) -> bool {
        if self.solving_row_upper {
            return false;
        }
        let candidates: Vec<_> = self
            .row_equalities
            .iter()
            .flat_map(|(a, b)| [self.zonk_row(a), self.zonk_row(b)])
            .collect();
        for upper in candidates {
            let mut metas = BTreeSet::new();
            self.metas(&Type::Errors(upper.clone()), &mut metas);
            if metas.is_empty() || !self.row_implied(actual, &upper) {
                continue;
            }
            let saved = self.clone();
            self.solving_row_upper = true;
            if self
                .within(&upper, allowed)
                .and_then(|()| self.settle())
                .is_ok()
                && self.row_implied(actual, allowed)
            {
                self.solving_row_upper = false;
                return true;
            }
            *self = saved;
        }
        false
    }

    pub(super) fn unify_caps(&mut self, a: &Caps, b: &Caps) -> Check<()> {
        let (a, b) = (self.zonk_caps(a), self.zonk_caps(b));
        if a == b {
            return Ok(());
        }
        for (x, y) in [(&a, &b), (&b, &a)] {
            if x.atoms.is_empty()
                && let [Var::Meta(m)] = x.vars.iter().collect::<Vec<_>>()[..]
                && !y.vars.contains(&Var::Meta(*m))
                && !y.atoms.contains(&Var::Meta(*m))
            {
                return self.bind_caps(*m, y.clone());
            }
        }
        if self.refining {
            self.cap_equalities.push((a, b));
        } else {
            let (a, b) = (Type::Caps(a), Type::Caps(b));
            self.hold(
                slice::from_ref(&a),
                slice::from_ref(&b),
                &Caps::default(),
                "≡",
            );
            self.hold(&[b], &[a], &Caps::default(), "≡");
        }
        Ok(())
    }

    pub(super) fn refine_row(&mut self, a: Row, b: Row) -> Check<()> {
        for (name, args) in &a.elems {
            if let Some(other) = b.elems.get(name) {
                if args.len() != other.len() {
                    return error("GADT", "row payload arities differ");
                }
                for (x, y) in args.iter().zip(other.iter()) {
                    self.unify(x, y)?;
                }
            }
        }
        self.row_equalities.push((a, b));
        Ok(())
    }
}
