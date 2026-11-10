//! Rows under the live discipline (`live.rs`): what a row needs live is
//! checked within its bound, discharged at the end of a `scope` and kept
//! by an owner. Within a bound, `Scope ϱ` keeps `ϱ` live as `Live {ϱ}`
//! does: they are the same row atoms (`formal/Formal/Live/Mode.lean`,
//! `TypingMode.use`; `Row.scope s = Row.ofCaps s.reach`). Not the other
//! way: `Live ϱ` gives no owner to acquire into. The `Scope` of work an
//! owner keeps is the owner's own, so it adds nothing to what must outlive
//! the owner (`keep_live`).

use super::live::merge;
use super::{LIVE, Outlives};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::effects::Site;
use crate::types::ty::{Norm, Row, Type};

impl Checker<'_> {
    /// The regions the `Scope` of the bound `services` keeps live.
    pub(in crate::types) fn scope_live(&self, services: &Norm) -> Norm {
        let Some(scope) = services.get(super::scope()) else {
            return Norm::default();
        };
        match scope.spine().1[..] {
            [region] => self.live_atoms(region, &[]),
            _ => Norm::default(),
        }
    }

    /// Unification of service rows: `Live` that one side has and the other,
    /// a closed row, lacks, is left out while it needs no region known yet,
    /// as of a cell's capacity a generic signature names.
    pub(in crate::types) fn unify_live(&self, left: &mut Norm, right: &mut Norm) {
        let lacks =
            |n: &Norm| n.get(LIVE).is_none() && !n.tails.iter().any(|t| matches!(t, Type::Meta(_)));
        let unknown = |this: &Self, t: &Type| {
            (this.live_atoms(t, &[]).entries.iter())
                .all(|(_, a)| matches!(this.store.resolve(a), Type::Meta(_)))
        };
        if lacks(right) {
            left.entries.retain(|(l, t)| l != LIVE || !unknown(self, t));
        }
        if lacks(left) {
            right
                .entries
                .retain(|(l, t)| l != LIVE || !unknown(self, t));
        }
    }

    /// The end of `scope` of `region`: its region is live no further.
    pub(in crate::types) fn discharge_live(&self, actual: &mut Norm, region: &Type) {
        if !actual.entries.iter().any(|(l, _)| l == LIVE) {
            return;
        }
        let region = self.store.resolve(region);
        let mut live = self.live_in(actual);
        actual.entries.retain(|(l, _)| l != LIVE);
        live.entries
            .retain(|(_, a)| self.store.resolve(a) != region);
        if !live.entries.is_empty() || !live.tails.is_empty() {
            let at = actual.entries.partition_point(|(l, _)| l.as_str() < LIVE);
            actual
                .entries
                .insert(at, (LIVE.to_owned(), live.into_type()));
        }
    }

    /// The regions `actual` needs live that `declared` does not.
    fn live_missing(&self, actual: &Norm, declared: &Norm) -> Vec<(String, Type)> {
        let (mine, mut theirs) = (self.live_in(actual), self.live_in(declared));
        merge(&mut theirs, self.scope_live(declared));
        mine.entries
            .into_iter()
            .filter(|(label, _)| theirs.get(label).is_none())
            .collect()
    }

    /// Grows what `declared` needs live by what `actual` needs and it lacks:
    /// a qualifier not inferred yet in its `Live`, else its open row `open`.
    pub(in crate::types) fn grow_live(
        &mut self,
        actual: &Norm,
        declared: &Norm,
        open: Option<u32>,
    ) -> bool {
        let mut missing = self.live_missing(actual, declared);
        missing.retain(|(_, a)| !matches!(self.store.resolve(a), Type::Meta(_)));
        if missing.is_empty() {
            return false;
        }
        let inner = (declared.entries.iter())
            .filter(|(l, _)| l == LIVE)
            .find_map(|(_, t)| {
                self.store
                    .normalize(t)
                    .tails
                    .into_iter()
                    .find_map(|t| match t {
                        Type::Meta(m) => Some(m),
                        _ => None,
                    })
            });
        let (m, grown) = match (inner, open) {
            (Some(m), _) => {
                let rest = self.store.fresh_beside(m);
                let row = Row {
                    entries: missing,
                    tail: Some(Box::new(rest)),
                    ..Row::default()
                };
                (m, Type::Row(row))
            }
            (None, Some(m)) => {
                let rest = self.store.fresh_beside(m);
                let live = Type::Row(Row::closed(missing));
                let row = Row {
                    entries: vec![(LIVE.to_owned(), live)],
                    tail: Some(Box::new(rest)),
                    ..Row::default()
                };
                (m, Type::Row(row))
            }
            (None, None) => return false,
        };
        self.bind(m, &grown).is_ok()
    }

    /// Once rows are solved: a region `actual` needs live and `declared`, the
    /// bound of the stage at `site`, does not keep live (`FLICK-G002`).
    pub(in crate::types) fn report_live(
        &mut self,
        actual: &Norm,
        declared: &Norm,
        site: usize,
        span: Span,
    ) {
        if declared.tails.contains(&Type::Error) {
            return;
        }
        let missing = self.live_missing(actual, declared);
        let Some(atom) = (missing.iter())
            .map(|(_, a)| self.store.resolve(a))
            .find(|a| !matches!(a, Type::Meta(_)))
        else {
            return;
        };
        let reported = |d: &Diagnostic| d.code == codes::G002 && d.span == Some(span);
        if self.diagnostics.iter().any(reported) {
            return;
        }
        let (region, scope) = self.describe(&atom);
        let bound = match &self.sites[site] {
            Site::Signature(name, _) => format!("the signature of `{name}`"),
            Site::Expected(ty, _) => format!("the expected type `{}`", self.show(ty)),
            Site::Inferred => "the type of this function".to_owned(),
        };
        let mut diagnostic = Diagnostic::error(
            codes::G002,
            format!(
                "this uses {region}, which {bound} does not keep live: a function or a task uses a region only while the region is live"
            ),
            span,
        );
        if let Some(scope) = scope {
            diagnostic = diagnostic.with_related(scope, "the region's scope ends here");
        }
        self.diagnostics.push(diagnostic.with_fix(|| {
            (
                "use the value inside the region's `scope`, or name the region in a parameter of the signature",
                Vec::new(),
            )
        }));
    }

    /// `Live` of the row of work an owner keeps (`Fiber.Provided`): each
    /// region it needs outlives the owner (`Outlives`, `FLICK-G003`).
    pub(in crate::types) fn keep_live(
        &mut self,
        live: &Type,
        owner: &Type,
        givens: &[Type],
        span: Span,
        last: bool,
    ) {
        let o = Outlives {
            captures: live.clone(),
            owner: owner.clone(),
            givens: givens.to_vec(),
            span,
        };
        if !last {
            self.regions.outlives.push(o);
        } else if !(self.diagnostics.iter()).any(|d| d.code == codes::G003 && d.span == Some(span))
        {
            self.check_outlives(&o);
        }
    }
}
