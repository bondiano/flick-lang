//! Solving `actual ⊆ declared` between rows. A declared row that is still
//! unknown grows by the labels and row variables flowing into it; what stays
//! unknown at the end is the empty row, the least solution. Then every
//! constraint either holds or names what its bound lacks.

use crate::types::Elem;
use std::mem;

use crate::source::Span;
use crate::types::gadt::row_facts::RowKind;
use crate::types::ty::{Norm, Row, Type, Written};
use crate::types::{Checker, regions};

/// The row `actual` of a call must be part of `declared`.
#[derive(Debug, Clone)]
pub struct SubRow {
    pub actual: Type,
    pub declared: Type,
    pub site: usize,
    pub span: Span,
    /// An error row; a service row otherwise.
    pub errors: bool,
    /// For the body of a `scope`: its region, whose `Scope` the scope
    /// provides. `Scope` of another region passes on to `declared`, the
    /// services around the scope; one of a region not known yet is the
    /// scope's own.
    pub provides: Option<Type>,
    /// The equalities of the GADT branches the call is in.
    pub givens: Vec<Type>,
}

/// What one bound lacks.
struct Excess {
    site: usize,
    span: Span,
    services: Vec<String>,
    errors: Vec<String>,
    /// The row labels of what is lacking, for the fixes that name a service.
    labels: Vec<Elem>,
    /// Where the bound's row is written, once rows are solved: a variable
    /// may stand for a row written elsewhere.
    bound: Written,
    /// A name the bound takes out of its row variable is lacking: writing
    /// it into the row is no fix.
    taken: bool,
}

mod excess;
mod report;
mod tails;

use tails::{growth_allows, less, owed, unknown};

pub(super) fn unknown_tail(tail: &Type) -> Option<u32> {
    unknown(tail)
}

impl Checker<'_> {
    /// Grows unknown declared rows until nothing changes.
    pub fn grow(&mut self) {
        loop {
            // `|`, not `||`: every row grows in each round
            let rows =
                (0..self.cx.subrows.len()).fold(false, |changed, i| changed | self.grow_one(i));
            if !(self.grow_holds() || rows) {
                break;
            }
        }
    }

    /// The actual row of constraint `i` without what its scope provides.
    fn actual(&mut self, i: usize) -> Norm {
        let mut actual = self.store.normalize(&self.cx.subrows[i].actual);
        let Some(region) = self.cx.subrows[i].provides.clone() else {
            return actual;
        };
        self.discharge_live(&mut actual, &region);
        let Some(scope) = actual.get(regions::scope()).cloned() else {
            return actual;
        };
        if let [owner] = scope.spine().1[..] {
            let owner = owner.clone();
            if let Type::Meta(_) = self.store.resolve(&owner) {
                let _ = self.unify(&owner, &region);
            }
            if self.store.resolve(&owner) == self.store.resolve(&region) {
                actual
                    .entries
                    .retain(|(label, _)| label != regions::scope());
            }
        }
        actual
    }

    /// Adds to the declared row of constraint `i` one label or row variable
    /// it lacks, if that row is open; `true` when it did.
    fn grow_one(&mut self, i: usize) -> bool {
        let givens = mem::take(&mut self.cx.subrows[i].givens);
        let grown = self.under(&givens, |this| this.grow_row(i));
        self.cx.subrows[i].givens = givens;
        grown
    }

    fn grow_row(&mut self, i: usize) -> bool {
        let actual = self.actual(i);
        let declared = self.store.normalize(&self.cx.subrows[i].declared);
        let kind = if self.cx.subrows[i].errors {
            RowKind::Errors
        } else {
            RowKind::Services
        };
        if !self.store.row_equalities.is_empty()
            && self.row_implied(
                &actual.clone().into_type(),
                &declared.clone().into_type(),
                kind,
            )
        {
            return false;
        }
        // a row still unknown, maybe less some names: it grows by what the
        // actual row holds, but never by a name it is taken without
        let unknown_tail = declared.tails.iter().find(|t| unknown(t).is_some());
        let open = unknown_tail.map(|tail| less(tail).0.clone());
        let never: Vec<Elem> = unknown_tail.map_or_else(Vec::new, |t| less(t).1.to_vec());
        if let Some(variable) = open.as_ref()
            && self.grow_scoped_row(&actual, kind, variable)
        {
            return true;
        }
        if self.grow_live(&actual, &declared, open.as_ref()) {
            return true;
        }
        for (label, ty) in actual
            .entries
            .iter()
            .filter(|(l, _)| !matches!(l, Elem::Live))
        {
            match declared.get(label) {
                Some(bound) => {
                    let bound = bound.clone();
                    let _ = self.unify(ty, &bound);
                }
                None => {
                    if let Some(variable) = open.as_ref().filter(|_| !never.contains(label)) {
                        let Some(rest) = self.store.fresh_beside_occurrence(variable) else {
                            return false;
                        };
                        let row = Type::Row(Row {
                            entries: vec![(label.clone(), ty.clone())],
                            tail: Some(Box::new(rest)),
                            ..Row::default()
                        });
                        return self.bind_inference(variable, &row).is_ok();
                    }
                }
            }
        }
        for tail in &actual.tails {
            if matches!(tail, Type::Error) || unknown(tail).is_some() {
                continue;
            }
            // a variable the declared row holds, less some names, is allowed,
            // or is owed names that growing the open variable would leave
            // owed too: that would add it again and again, so what it lacks
            // is reported instead
            if let Some(names) = owed(&declared, tail)
                && (names.is_empty() || !growth_allows(&declared, tail, &never))
            {
                continue;
            }
            if let Some(variable) = open.as_ref() {
                let Some(rest) = self.store.fresh_beside_occurrence(variable) else {
                    return false;
                };
                let union = Type::Union(Box::new(tail.clone()), Box::new(rest));
                return self.bind_inference(variable, &union).is_ok();
            }
        }
        false
    }

    /// Before a binding is generalized: solves the constraints so far and
    /// settles the unknown rows of its calls and its widened rows, so they
    /// become no row variables of its type. A widened row is closed. The row
    /// of a call is closed at the top level; inside a function its constraint
    /// stays pending and of one type, so the uses of the binding grow it and
    /// the end of the declaration closes it, as for a binding not generalized.
    /// Type arguments of labels whose inclusion already holds can generalize.
    pub fn settle_rows(&mut self) {
        self.grow();
        let level = self.store.level;
        let deep = |tail: &Type| {
            unknown(tail).filter(|m| self.store.meta_level(*m).is_some_and(|l| l > level))
        };
        let mut close = Vec::new();
        let mut pending = Vec::new();
        for c in &self.cx.subrows {
            let actual = self.store.normalize(&c.actual);
            if level == 0 {
                close.extend(actual.tails.iter().filter_map(deep));
                continue;
            }
            let declared = self.store.normalize(&c.declared);
            let declared_deep = || declared.tails.iter().filter(|tail| deep(tail).is_some());
            if !actual.tails.iter().any(|tail| deep(tail).is_some())
                && declared_deep().next().is_none()
            {
                continue;
            }
            // Matching entries do not need to be lowered once the actual
            // row cannot gain another label from a pending call.
            let closed = actual.tails.is_empty();
            let scoped = c.provides.is_some();
            if (closed || scoped)
                && (!scoped || actual.get(regions::scope()).is_none())
                && self.matched_call_labels(&actual, &declared)
                && (closed || self.scope_tail_settled(&actual.tails))
            {
                pending.extend(
                    actual
                        .tails
                        .iter()
                        .filter(|tail| deep(tail).is_some())
                        .cloned(),
                );
                pending.extend(declared_deep().cloned());
            } else {
                pending.extend([c.actual.clone(), c.declared.clone()]);
            }
        }
        for row in &self.cx.widened_rows {
            close.extend(self.store.normalize(row).tails.iter().filter_map(deep));
        }
        for m in close {
            if self.store.meta_level(m).is_some() {
                self.store.set(m, Type::empty_row());
            }
        }
        for side in pending {
            self.store.lower(&side);
        }
        self.close_holds(Some(level));
    }

    /// The actual labels are fixed and their payloads already agree with the bound.
    fn matched_call_labels(&self, actual: &Norm, declared: &Norm) -> bool {
        actual.entries.iter().all(|(label, ty)| {
            matches!(label, Elem::Named(_))
                && declared
                    .get(label)
                    .is_some_and(|bound| self.store.zonk(ty) == self.store.zonk(bound))
        })
    }

    /// A scope's inferred body row can stay open after all its calls have
    /// settled. A call with an open or unmatched actual row can still grow
    /// that tail, including when it forwards another scope's open row.
    fn scope_tail_settled(&self, tails: &[Type]) -> bool {
        let [tail] = tails else {
            return false;
        };
        let Some(root) = unknown(tail) else {
            return false;
        };
        let mut settled = false;
        for c in self.cx.subrows.iter().filter(|c| !c.errors) {
            let declared = self.store.normalize(&c.declared);
            if !declared
                .tails
                .iter()
                .any(|tail| unknown(tail) == Some(root))
            {
                continue;
            }
            let actual = self.store.normalize(&c.actual);
            if !actual.tails.is_empty() || !self.matched_call_labels(&actual, &declared) {
                return false;
            }
            settled = true;
        }
        settled
    }

    /// At the end of a declaration: solves its constraints, makes what is
    /// still unknown empty, and reports every bound that lacks something.
    pub fn solve_rows(&mut self) {
        self.close_rows();
        self.report_rows();
        self.report_regions();
    }

    /// Solves the constraints so far and makes every row they leave unknown
    /// empty.
    pub fn close_rows(&mut self) {
        self.close_rows_above(None);
    }

    /// `close_rows` for the rows of variables deeper than `level` only: a
    /// binding's own, while an enclosing declaration still gathers its calls.
    pub fn close_rows_above(&mut self, level: Option<u32>) {
        self.grow();
        let mut open = Vec::new();
        for c in &self.cx.subrows {
            for side in [&c.actual, &c.declared] {
                for t in self.store.normalize(side).tails {
                    open.extend(unknown(&t));
                }
            }
        }
        for row in &self.cx.widened_rows {
            for t in self.store.normalize(row).tails {
                open.extend(unknown(&t));
            }
        }
        for m in open {
            if self
                .store
                .meta_level(m)
                .is_some_and(|l| level.is_none_or(|level| l > level))
            {
                self.store.set(m, Type::empty_row());
            }
        }
        self.close_holds(level);
    }

    fn report_rows(&mut self) {
        let mut excess: Vec<Excess> = Vec::new();
        for i in 0..self.cx.subrows.len() {
            let givens = self.cx.subrows[i].givens.clone();
            self.under(&givens, |this| this.collect_excess(i, &mut excess));
        }
        self.cx.subrows.clear();
        for e in excess {
            self.report_excess(&e);
        }
    }

    /// Report under the same branch assumptions used while growing the row.
    fn collect_excess(&mut self, i: usize, excess: &mut Vec<Excess>) {
        let actual = self.actual(i);
        let c = self.cx.subrows[i].clone();
        let declared = self.store.normalize(&c.declared);
        let kind = if c.errors {
            RowKind::Errors
        } else {
            RowKind::Services
        };
        if !self.store.row_equalities.is_empty()
            && self.row_implied(
                &actual.clone().into_type(),
                &declared.clone().into_type(),
                kind,
            )
        {
            return;
        }
        // a declared element that is no error is reported as such, once
        if c.errors && declared.entries.iter().any(|(_, t)| !self.may_be_error(t)) {
            return;
        }
        let mut lacking = Vec::new();
        let mut labels = Vec::new();
        self.report_live(&actual, &declared, c.site, c.span);
        for (label, ty) in actual
            .entries
            .iter()
            .filter(|(l, _)| !matches!(l, Elem::Live))
        {
            match declared.get(label) {
                Some(bound) => {
                    if let Err(m) = self.unify(ty, bound) {
                        self.mismatch(&m, ty, bound, c.span);
                    }
                }
                None if label == regions::scope() => self.no_owner(ty, &c),
                None if matches!(label, Elem::Provided(_)) => {
                    // the element names one service (`regions::provided`
                    // `obligation`), never a row variable: only its owner can be unsigned
                    if let [service, owner] = ty.spine().1[..] {
                        let (service, named) = (self.show(service), self.signed(owner));
                        let owner = self.show(owner);
                        let service = service.trim_matches(['{', '}', ' ']);
                        self.unprovided(service, &owner, named, c.span);
                    }
                }
                None => {
                    lacking.push(self.show(ty));
                    labels.push(label.clone());
                }
            }
        }
        let mut taken = false;
        for tail in (actual.tails.iter()).filter(|t| !matches!(t, Type::Error)) {
            match owed(&declared, tail) {
                // the variable is allowed, less names nothing gives back
                Some(names) => {
                    // an instance of a service the row holds itself, which
                    // would answer one the variable holds at other arguments
                    let (answered, names): (Vec<Elem>, Vec<Elem>) =
                        names.into_iter().partition(|l| declared.get(l).is_some());
                    for l in answered {
                        self.instance_answered(&l, &declared, tail, c.span);
                    }
                    taken |= !names.is_empty();
                    lacking.extend(names.iter().map(Elem::short));
                    labels.extend(names);
                }
                None => lacking.push(self.show(tail)),
            }
        }
        if declared.tails.contains(&Type::Error) || lacking.is_empty() {
            return;
        }
        let bound = if c.errors {
            Written::default()
        } else {
            self.local_place_at(&c.declared, c.span)
        };
        let (services, errors) = if c.errors {
            (Vec::new(), lacking)
        } else {
            (lacking, Vec::new())
        };
        let next = Excess {
            site: c.site,
            span: c.span,
            services,
            errors,
            labels,
            bound,
            taken,
        };
        if let Some(earlier) = excess.iter_mut().find(|e| e.site == c.site) {
            earlier.merge(next);
        } else {
            excess.push(next);
        }
    }
}
