//! Variance-directed instance matching (B1) and symmetric K1' overlap checking
//! (`docs/formal/regions.md` §2.2.1, `docs/formal/elaboration.md` §4, Lean
//! `Formal/Elab/{Variance,Hyp,Coherence}.lean`).
//!
//! Functional-dependency improvement (`improve_by_instances`) and `FLICK-C004`
//! consistency stay strictly equality-based (`matching::match_type`,
//! `matching::unify_heads`). This module handles only variance-directed
//! dictionary matching (`Cn.Serves`) and K1' common-query overlap detection.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};

use super::matching::{Match, Subst, match_type};
use super::split;
use crate::types::Checker;
use crate::types::ty::{Binder, Norm, Type};
use crate::types::variance::Variance;

mod caps;
mod collect;
mod overlap;
mod verify;

use caps::{norm_has_meta_tail, norm_within, reach_within, row_without_live_norm, split_caps_cow};
use overlap::{OverlapMode, OverlapSolver};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Dir {
    Eq,
    /// `pattern[σ] ≤ target` (`+` class parameter).
    Sub,
    /// `target ≤ pattern[σ]` (`-` class parameter).
    Sup,
}

impl Dir {
    pub(super) fn flip(self) -> Self {
        match self {
            Dir::Eq => Dir::Eq,
            Dir::Sub => Dir::Sup,
            Dir::Sup => Dir::Sub,
        }
    }
}

#[derive(Default)]
pub(super) struct VarBounds {
    pub(super) exact: HashMap<String, Type>,
    pub(super) lower: HashMap<String, Vec<Type>>,
    pub(super) upper: HashMap<String, Vec<Type>>,
    pub(super) rows: HashMap<String, Type>,
    pub(super) caps: HashMap<String, Norm>,
}

impl Checker<'_> {
    /// Variance of each parameter of class `(module, name)`, falling back to
    /// `Invariant` when metadata is missing.
    pub(super) fn class_variances(
        &mut self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Vec<Variance> {
        self.class_signs(module, name)
            .filter(|s| s.len() == arity)
            .map_or_else(|| vec![Variance::Invariant; arity], |s| s.to_vec())
    }

    /// Data or explicit `$Dict` variance for a nominal constructor.
    pub(super) fn app_variances(
        &mut self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Option<Vec<Variance>> {
        if let Some(class_name) = name.strip_suffix("$Dict")
            && self.class_info(module, class_name).is_some()
        {
            return self
                .class_signs(module, class_name)
                .filter(|s| s.len() == arity)
                .map(|s| s.to_vec());
        }
        self.signs(module, name)
            .filter(|s| s.len() == arity)
            .map(|s| s.to_vec())
    }

    /// Whether a given constraint `given` (no head variables to instantiate)
    /// serves `query` under the class's inferred parameter variances.
    pub(super) fn given_serves(&mut self, query: &Type, given: &Type) -> Match {
        let (Some(((qm, qn), q_args)), Some(((gm, gn), g_args))) = (split(query), split(given))
        else {
            return Match::No;
        };
        if qm != gm || qn != gn || q_args.len() != g_args.len() {
            return Match::No;
        }
        let signs = self.class_variances(qm, qn, q_args.len());
        let mut subst = Subst::new();
        self.match_args_variance(&signs, &[], &g_args, &q_args, &mut subst)
    }

    /// Direction-aware matching of an instance head against query `args` using
    /// precomputed class parameter `signs`.
    ///
    /// - `Covariant` (`+`): `head[σ] ≤ query`
    /// - `Contravariant` (`-`): `query ≤ head[σ]` and `⌈head[σ]⌉ ⊆ ⌈query⌉`
    /// - `Invariant` / `Bivariant`: exact equality (`match_type`)
    pub(super) fn match_head_variance_with_signs(
        &mut self,
        signs: &[Variance],
        head_vars: &[Binder],
        head: &Type,
        args: &[Type],
        subst: &mut Subst,
    ) -> Match {
        let Some((_, pattern)) = split(head) else {
            return Match::No;
        };
        if pattern.len() != args.len() {
            return Match::No;
        }
        self.match_args_variance(signs, head_vars, &pattern, args, subst)
    }

    fn match_args_variance<P: Borrow<Type>, A: Borrow<Type>>(
        &mut self,
        signs: &[Variance],
        head_vars: &[Binder],
        pattern: &[P],
        args: &[A],
        subst: &mut Subst,
    ) -> Match {
        // Fast path: all invariant/bivariant matches via exact equality.
        if signs
            .iter()
            .all(|v| matches!(v, Variance::Invariant | Variance::Bivariant))
        {
            if head_vars.is_empty() {
                return pattern.iter().zip(args).fold(Match::Yes, |m, (p, t)| {
                    m.and(|| self.match_rigid_eq(p.borrow(), t.borrow()))
                });
            }
            return pattern.iter().zip(args).fold(Match::Yes, |m, (p, t)| {
                m.and(|| match_type(p.borrow(), t.borrow(), subst))
            });
        }

        let mut bounds = VarBounds::default();
        let mut outcome = Match::Yes;
        for ((p, t), sign) in pattern.iter().zip(args).zip(signs) {
            let dir = match sign {
                Variance::Covariant => Dir::Sub,
                Variance::Contravariant => Dir::Sup,
                Variance::Invariant | Variance::Bivariant => Dir::Eq,
            };
            let step = self.collect_match(head_vars, p.borrow(), t.borrow(), dir, &mut bounds);
            outcome = outcome.and(|| step);
            if outcome == Match::No {
                return Match::No;
            }
        }
        if outcome != Match::Yes {
            return outcome;
        }

        let Some(solved) = self.solve_var_bounds(head_vars, bounds) else {
            return Match::No;
        };

        // Final verification against `Cn.Serves`: confirms subtyping AND
        // reach preservation (`SafeUse`) for every `-` and `+` argument.
        let mut final_match = Match::Yes;
        for ((p, q), sign) in pattern.iter().zip(args).zip(signs) {
            let q = q.borrow();
            let h = p.borrow().substitute(&solved);
            if h == *q {
                continue;
            }
            let step = match sign {
                Variance::Covariant => self.verify_ty_le(&h, q),
                Variance::Contravariant => {
                    self.verify_ty_le(q, &h).and(|| reach_within(self, &h, q))
                }
                Variance::Invariant | Variance::Bivariant => self.match_rigid_eq(&h, q),
            };
            final_match = final_match.and(|| step);
            if final_match == Match::No {
                return Match::No;
            }
        }
        if final_match == Match::Yes {
            subst.extend(solved);
        }
        final_match
    }

    /// Exact equality match when `pattern` has no head variables to bind (e.g.
    /// a given constraint or post-substitution verification).
    pub(super) fn match_rigid_eq(&self, a: &Type, b: &Type) -> Match {
        if matches!(a, Type::Error) || matches!(b, Type::Error) {
            return Match::Yes;
        }
        if a == b {
            return Match::Yes;
        }
        let (ba, ca) = split_caps_cow(self, a);
        let (bb, cb) = split_caps_cow(self, b);
        if matches!(&*ba, Type::Meta(_)) || matches!(&*bb, Type::Meta(_)) {
            return Match::Unknown;
        }
        let caps_m = norm_within(&ca, &cb).and(|| norm_within(&cb, &ca));
        if caps_m == Match::No {
            return Match::No;
        }
        let body_m = match (&*ba, &*bb) {
            (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => self
                .match_rigid_eq(h1, h2)
                .and(|| self.match_rigid_eq(a1, a2)),
            (
                Type::Fun {
                    param: p1,
                    effects: e1,
                    result: r1,
                },
                Type::Fun {
                    param: p2,
                    effects: e2,
                    result: r2,
                },
            ) => {
                let eff = match (e1, e2) {
                    (None, None) => Match::Yes,
                    (Some(x), Some(y)) if x.sync == y.sync => self
                        .match_rigid_row_eq(&x.services, &y.services)
                        .and(|| self.match_rigid_row_eq(&x.errors, &y.errors)),
                    _ => Match::No,
                };
                eff.and(|| self.match_rigid_eq(p1, p2))
                    .and(|| self.match_rigid_eq(r1, r2))
            }
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys)
                .fold(Match::Yes, |m, (x, y)| m.and(|| self.match_rigid_eq(x, y))),
            (Type::Record(r1), Type::Record(r2)) | (Type::Row(r1), Type::Row(r2)) => {
                self.match_rigid_row_eq(&Type::Row(r1.clone()), &Type::Row(r2.clone()))
            }
            _ if *ba == *bb => Match::Yes,
            _ => Match::No,
        };
        caps_m.and(|| body_m)
    }

    pub(super) fn match_rigid_row_eq(&self, a: &Type, b: &Type) -> Match {
        let na = row_without_live_norm(self, a);
        let nb = row_without_live_norm(self, b);
        let a_meta = norm_has_meta_tail(&na);
        let b_meta = norm_has_meta_tail(&nb);
        let mut res = Match::Yes;
        for (l, ta) in &na.entries {
            match nb.entries.iter().find(|(lb, _)| lb == l) {
                Some((_, tb)) => res = res.and(|| self.match_rigid_eq(ta, tb)),
                None if b_meta => res = res.and(|| Match::Unknown),
                None => return Match::No,
            }
        }
        for (l, _) in &nb.entries {
            if !na.entries.iter().any(|(la, _)| la == l) {
                if a_meta {
                    res = res.and(|| Match::Unknown);
                } else {
                    return Match::No;
                }
            }
        }
        if na.tails != nb.tails {
            if a_meta || b_meta {
                res = res.and(|| Match::Unknown);
            } else {
                return Match::No;
            }
        }
        res
    }
}

fn freshen_args(
    args: &[Type],
    vars: &[Binder],
    prefix: &str,
) -> (HashSet<String>, Vec<Type>, Subst) {
    let mut fresh_vars = HashSet::new();
    let mut forward = Subst::new();
    let mut back = Subst::new();
    for (i, b) in vars.iter().enumerate() {
        let fresh = format!("{prefix}${i}");
        fresh_vars.insert(fresh.clone());
        forward.push((b.name.clone(), Type::Var(fresh.clone())));
        let display = if prefix == "L" {
            b.name.clone()
        } else {
            format!("{}'", b.name)
        };
        back.push((fresh, Type::Var(display)));
    }
    let renamed = args.iter().map(|a| a.substitute(&forward)).collect();
    (fresh_vars, renamed, back)
}

impl Checker<'_> {
    /// Symmetric K1' overlap check for two instances of `class`.
    ///
    /// Returns `Some(common_query)` when the two heads can both serve a common
    /// query under `class`'s inferred parameter variances, or `None` when no
    /// common query exists.
    pub(super) fn heads_overlap_k1_prime(
        &mut self,
        class: (&str, &str),
        left_vars: &[Binder],
        left_args: &[Type],
        right_vars: &[Binder],
        right_args: &[Type],
    ) -> Option<Type> {
        if left_args.len() != right_args.len() {
            return None;
        }
        let signs = self.class_variances(class.0, class.1, left_args.len());
        let (mut vars, l_args, mut back) = freshen_args(left_args, left_vars, "L");
        let (r_vars, r_args, r_back) = freshen_args(right_args, right_vars, "R");
        vars.extend(r_vars);
        back.extend(r_back);

        let mut solver = OverlapSolver::new(self, vars);
        for ((x, y), &sign) in l_args.iter().zip(&r_args).zip(&signs) {
            let mode = match sign {
                Variance::Covariant => OverlapMode::CommonUpper,
                Variance::Contravariant => OverlapMode::CommonLowerReach,
                Variance::Invariant | Variance::Bivariant => OverlapMode::Equal,
            };
            solver.queue.push_back((mode, x.clone(), y.clone()));
        }
        if !solver.solve() {
            return None;
        }

        // Build a human-readable common query witness for `FLICK-C003`.
        let l_applied: Vec<Type> = l_args
            .iter()
            .map(|a| solver.apply_bindings(a).substitute(&back))
            .collect();
        let r_applied: Vec<Type> = r_args
            .iter()
            .map(|a| solver.apply_bindings(a).substitute(&back))
            .collect();
        drop(solver);

        let mut common_args = Vec::with_capacity(l_applied.len());
        for ((l, r), sign) in l_applied.into_iter().zip(r_applied).zip(&signs) {
            let witness = match sign {
                Variance::Covariant => self.join_ty(&l, &r).unwrap_or(l),
                Variance::Contravariant => self.meet_ty(&l, &r).unwrap_or(l),
                Variance::Invariant | Variance::Bivariant => l,
            };
            common_args.push(witness);
        }
        Some(Type::apps(Type::con(class.0, class.1), common_args))
    }
}
