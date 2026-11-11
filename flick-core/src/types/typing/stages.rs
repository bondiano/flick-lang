//! The stages of a written function type, and how one stage lifts into
//! another where another is expected.

use super::super::Checker;
use super::super::regions::{AliasKey, Unfold, leave_alias};
use super::super::ty::{Stage, Type, stage_of};
use super::Lift;

impl Checker<'_> {
    /// The runtime arrows, through aliases, quantifiers and capture qualifiers.
    pub fn arrow_stages(&self, ty: &Type) -> Vec<Stage> {
        let mut out = Vec::new();
        let mut ty = ty.clone();
        loop {
            match self.qualified_head(&ty) {
                Type::Forall { body, .. }
                | Type::Captured { ty: body, .. }
                | Type::Bounded { ty: body, .. } => ty = *body,
                Type::Fun {
                    effects, result, ..
                } => {
                    out.push(stage_of(effects.as_deref()));
                    ty = *result;
                }
                _ => break,
            }
        }
        out
    }

    /// How `actual` lifts into `expected`, if any stage needs it.
    pub fn lift(&self, actual: &Type, expected: &Type) -> Option<Lift> {
        self.lift_in(actual, expected, &mut Vec::new())
    }

    /// `path` holds the aliases both sides unfold: one met again inside its
    /// own unfolding lifts nothing, being the same type on both.
    fn lift_in(&self, actual: &Type, expected: &Type, path: &mut Vec<AliasKey>) -> Option<Lift> {
        let unfold = match (self.alias_key(actual), self.alias_key(expected)) {
            (Some(a), Some(e)) if a == e => self.enter_alias(actual, path),
            _ => Unfold::Plain,
        };
        if matches!(unfold, Unfold::Again) {
            return None;
        }
        let lifted = self.lift_unfolded(actual, expected, path);
        leave_alias(path, unfold);
        lifted
    }

    fn lift_unfolded(
        &self,
        actual: &Type,
        expected: &Type,
        path: &mut Vec<AliasKey>,
    ) -> Option<Lift> {
        let (a, e) = (self.whnf_captured(actual).0, self.whnf_captured(expected).0);
        match (&a, &e) {
            (
                Type::Fun {
                    effects: e1,
                    result: r1,
                    ..
                },
                Type::Fun {
                    effects: e2,
                    result: r2,
                    ..
                },
            ) => {
                let (from, to) = (stage_of(e1.as_deref()), stage_of(e2.as_deref()));
                // a pure function is called as a Sync one is
                let lift = from != to && (from, to) != (Stage::Pure, Stage::Sync);
                let result = self.lift_in(r1, r2, path).map(Box::new);
                (lift || result.is_some()).then_some(Lift::Stage { from, to, result })
            }
            _ => match (self.eff_parts(&a), self.eff_parts(&e)) {
                (Some((_, _, r1)), Some((_, _, r2))) => {
                    self.lift_in(&r1, &r2, path).map(|l| Lift::Eff(Box::new(l)))
                }
                _ => None,
            },
        }
    }
}
