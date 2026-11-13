//! A row has one instance of each nominal service/error. Preserve the
//! pre-normalization shape so substituting a tail cannot silently discard
//! another instance's invariant arguments. Union is not subtraction.

use std::collections::BTreeSet;

use super::{Check, Checker, Solved};
use crate::syntax::Path;
use crate::types::{Row, Stage, Type};

impl Checker<'_> {
    pub(super) fn remember_row_instances(&mut self, ty: &Type) {
        let mut rows = Vec::new();
        collect(ty, &mut rows);
        let facts = self.branch_givens();
        self.row_instances.extend(rows.into_iter().filter_map(|r| {
            (!r.elems.is_empty() && !r.vars.is_empty()).then(|| (r, facts.clone()))
        }));
    }

    pub(super) fn check_row_instances(&mut self) -> Check<()> {
        // Unifying instance arguments can solve a row nested in another
        // argument. Revisit the original shapes until no more slots change.
        loop {
            let solved = self.slots.iter().filter(|s| s.solved.is_some()).count();
            for (row, facts) in self.row_instances.clone() {
                self.under_givens(&facts, |c| c.instance_row(&row, &BTreeSet::new()))?;
            }
            self.settle()?;
            if solved == self.slots.iter().filter(|s| s.solved.is_some()).count() {
                return Ok(());
            }
        }
    }

    fn instance_row(&mut self, row: &Row, gone: &BTreeSet<Path>) -> Check<Row> {
        let mut result = Row {
            elems: row
                .elems
                .iter()
                .filter(|(n, _)| !gone.contains(*n))
                .map(|(n, a)| (n.clone(), a.clone()))
                .collect(),
            ..Row::none()
        };
        for var in &row.vars {
            let removed = gone.iter().chain(row.less_of(var)).cloned().collect();
            if let Some(Solved::Row(inner)) = self.solution(var).cloned() {
                let inner = self.instance_row(&inner, &removed)?;
                result = self.sum(&result, &inner)?;
            } else {
                result.add_var(var, &removed);
            }
        }
        Ok(result)
    }
}

fn collect(ty: &Type, out: &mut Vec<Row>) {
    let mut row = |r: &Row| {
        out.push(r.clone());
        for a in r.elems.values().flat_map(|args| args.iter()) {
            collect(a, out);
        }
    };
    match ty {
        Type::Fun(a, stage, b) => {
            if let Stage::Runtime(r, e) = stage {
                row(r);
                row(e);
            }
            collect(a, out);
            collect(b, out);
        }
        Type::Eff(r, e, x) => {
            row(r);
            row(e);
            collect(x, out);
        }
        Type::EffOf(r, e) => {
            row(r);
            row(e);
        }
        Type::Layer(i, e, o) => {
            row(i);
            row(e);
            row(o);
        }
        Type::Errors(r) | Type::Impl(r) | Type::Tag(r) | Type::Env(r) => row(r),
        Type::App(a, b) => {
            collect(a, out);
            collect(b, out);
        }
        Type::Captured(x, _) | Type::Bounded(x, _) | Type::Ref(x) => collect(x, out),
        Type::Record(f) => f.fields.values().for_each(|t| collect(t, out)),
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().for_each(|t| collect(t, out)),
        // A quantified body is checked at its own instance, not in this scope.
        Type::Forall(_) | Type::Var(_) | Type::Con(_) | Type::Caps(_) => {}
    }
}
