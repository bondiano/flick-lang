//! Variance-directed bound collection for instance head matching.

use super::caps::{
    is_head_var, live_caps_in_row, match_caps_dir, merge_norm, norm_has_meta_tail, norm_is_empty,
    norm_within, row_without_live_norm, split_caps_cow, stage_norm,
};
use super::{Dir, VarBounds};
use crate::types::Checker;
use crate::types::classes::has_metas;
use crate::types::classes::matching::Match;
use crate::types::elem::Elem;
use crate::types::lang::Lang;
use crate::types::ty::{Binder, Effects, Norm, Stage, Type, stage_of};
use crate::types::variance::Variance;

impl Checker<'_> {
    pub(super) fn collect_match(
        &mut self,
        head_vars: &[Binder],
        pat: &Type,
        target: &Type,
        dir: Dir,
        bounds: &mut VarBounds,
    ) -> Match {
        if matches!(pat, Type::Error) || matches!(target, Type::Error) {
            return Match::Yes;
        }
        // Head variable binds the whole target (including any qualifier).
        if let Type::Var(v) = pat
            && is_head_var(head_vars, v)
        {
            return self.bind_head_var(v, target, dir, bounds);
        }

        let (p_bare, p_caps) = split_caps_cow(self, pat);
        let (t_bare, t_caps) = split_caps_cow(self, target);
        if matches!(&*t_bare, Type::Meta(_)) {
            return Match::Unknown;
        }
        let caps_m = match_caps_dir(head_vars, &p_caps, &t_caps, dir, bounds);
        if caps_m == Match::No {
            return Match::No;
        }

        let body_m = match (&*p_bare, &*t_bare) {
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
                let stage_m = self.match_stage_dir(
                    head_vars,
                    e1.as_deref(),
                    &p_caps,
                    e2.as_deref(),
                    &t_caps,
                    dir,
                    bounds,
                );
                stage_m
                    .and(|| self.collect_match(head_vars, p1, p2, dir.flip(), bounds))
                    .and(|| self.collect_match(head_vars, r1, r2, dir, bounds))
            }
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
                xs.iter().zip(ys).fold(Match::Yes, |m, (x, y)| {
                    m.and(|| self.collect_match(head_vars, x, y, dir, bounds))
                })
            }
            (Type::Record(r1), Type::Record(r2)) => {
                if dir != Dir::Eq
                    && r1.tail.is_none()
                    && r2.tail.is_none()
                    && r1.entries.len() == r2.entries.len()
                    && r1.entries.iter().all(|(l, _)| r2.get(l).is_some())
                {
                    r1.entries.iter().fold(Match::Yes, |m, (l, x)| {
                        m.and(|| {
                            r2.get(l).map_or(Match::No, |y| {
                                self.collect_match(head_vars, x, y, dir, bounds)
                            })
                        })
                    })
                } else {
                    self.match_row_dir(
                        head_vars,
                        &Type::Row(r1.clone()),
                        &Type::Row(r2.clone()),
                        Dir::Eq,
                        false,
                        bounds,
                    )
                }
            }
            (Type::Row(_) | Type::Union(..), Type::Row(_) | Type::Union(..)) => {
                self.match_row_dir(head_vars, &p_bare, &t_bare, dir, false, bounds)
            }
            (Type::Var(v), _) if is_head_var(head_vars, v) => {
                self.bind_head_var(v, &t_bare, dir, bounds)
            }
            (Type::App { .. }, Type::App { .. }) => {
                self.collect_match_app(head_vars, &p_bare, &t_bare, dir, bounds)
            }
            _ if *p_bare == *t_bare => Match::Yes,
            _ => Match::No,
        };
        caps_m.and(|| body_m)
    }

    fn bind_head_var(&self, v: &str, target: &Type, dir: Dir, bounds: &mut VarBounds) -> Match {
        match dir {
            Dir::Eq => {
                if let Some(prev) = bounds.exact.get(v) {
                    return self.match_rigid_eq(prev, target);
                }
                bounds.exact.insert(v.to_owned(), target.clone());
                Match::Yes
            }
            Dir::Sub => {
                if let Some(existing) = bounds.upper.get(v)
                    && existing
                        .iter()
                        .any(|prev| prev != target && (has_metas(prev) || has_metas(target)))
                {
                    return Match::Unknown;
                }
                bounds
                    .upper
                    .entry(v.to_owned())
                    .or_default()
                    .push(target.clone());
                Match::Yes
            }
            Dir::Sup => {
                if let Some(existing) = bounds.lower.get(v)
                    && existing
                        .iter()
                        .any(|prev| prev != target && (has_metas(prev) || has_metas(target)))
                {
                    return Match::Unknown;
                }
                bounds
                    .lower
                    .entry(v.to_owned())
                    .or_default()
                    .push(target.clone());
                Match::Yes
            }
        }
    }

    fn collect_match_app(
        &mut self,
        head_vars: &[Binder],
        p_bare: &Type,
        t_bare: &Type,
        dir: Dir,
        bounds: &mut VarBounds,
    ) -> Match {
        let (p_head, p_args) = p_bare.spine();
        let (t_head, t_args) = t_bare.spine();
        if p_args.len() != t_args.len() {
            return Match::No;
        }
        // `Eff s e r`
        if p_head.is_lang(Lang::Eff)
            && t_head.is_lang(Lang::Eff)
            && let ([s1, e1, r1], [s2, e2, r2]) = (&p_args[..], &t_args[..])
        {
            return self
                .match_row_dir(head_vars, s1, s2, dir, true, bounds)
                .and(|| self.match_row_dir(head_vars, e1, e2, dir, false, bounds))
                .and(|| self.collect_match(head_vars, r1, r2, dir, bounds));
        }
        // `Layer i e o`
        if p_head.is_lang(Lang::Layer)
            && t_head.is_lang(Lang::Layer)
            && let ([i1, e1, o1], [i2, e2, o2]) = (&p_args[..], &t_args[..])
        {
            return self
                .match_row_dir(head_vars, i1, i2, dir, true, bounds)
                .and(|| self.match_row_dir(head_vars, e1, e2, dir, false, bounds))
                .and(|| self.collect_match(head_vars, o1, o2, Dir::Eq, bounds));
        }
        // Nominal data or explicit `$Dict`
        if let (
            Type::Con {
                module: m1,
                name: n1,
            },
            Type::Con {
                module: m2,
                name: n2,
            },
        ) = (p_head, t_head)
        {
            if m1 != m2 || n1 != n2 {
                return Match::No;
            }
            let signs = self.app_variances(m1, n1, p_args.len());
            return p_args
                .iter()
                .zip(&t_args)
                .enumerate()
                .fold(Match::Yes, |m, (j, (p, t))| {
                    m.and(|| {
                        let sign = signs
                            .as_ref()
                            .and_then(|s| s.get(j))
                            .copied()
                            .unwrap_or(Variance::Invariant);
                        match (dir, sign) {
                            (Dir::Eq, _) | (_, Variance::Invariant | Variance::Bivariant) => {
                                self.collect_match(head_vars, p, t, Dir::Eq, bounds)
                            }
                            (_, Variance::Covariant) => {
                                self.collect_match(head_vars, p, t, dir, bounds)
                            }
                            (_, Variance::Contravariant) => {
                                self.collect_match(head_vars, p, t, dir.flip(), bounds)
                            }
                        }
                    })
                });
        }
        // Higher-kinded application `f a` vs `g b`: always exact equality.
        self.collect_match(head_vars, p_head, t_head, Dir::Eq, bounds)
            .and(|| {
                p_args.iter().zip(&t_args).fold(Match::Yes, |m, (p, t)| {
                    m.and(|| self.collect_match(head_vars, p, t, Dir::Eq, bounds))
                })
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn match_stage_dir(
        &mut self,
        head_vars: &[Binder],
        e1: Option<&Effects>,
        p_caps: &Norm,
        e2: Option<&Effects>,
        t_caps: &Norm,
        dir: Dir,
        bounds: &mut VarBounds,
    ) -> Match {
        let (s1, s2) = (stage_of(e1), stage_of(e2));
        match (dir, e1, e2) {
            (Dir::Eq, None, None) | (Dir::Sub, None, _) | (Dir::Sup, _, None) => Match::Yes,
            (Dir::Eq, Some(x), Some(y)) if x.sync == y.sync => self
                .match_row_dir(head_vars, &x.services, &y.services, Dir::Eq, true, bounds)
                .and(|| {
                    self.match_row_dir(head_vars, &x.errors, &y.errors, Dir::Eq, false, bounds)
                }),
            (Dir::Sub, Some(x), Some(y)) => {
                if s1 == Stage::Runtime && s2 == Stage::Sync {
                    return Match::No;
                }
                let mut y_live = live_caps_in_row(self, &y.services);
                merge_norm(&mut y_live, t_caps);
                let mut x_live = live_caps_in_row(self, &x.services);
                merge_norm(&mut x_live, p_caps);
                let live_ok = if norm_is_empty(&x_live) {
                    Match::Yes
                } else {
                    norm_within(&x_live, &y_live)
                };
                live_ok
                    .and(|| {
                        self.match_row_dir(
                            head_vars,
                            &x.services,
                            &y.services,
                            Dir::Sub,
                            true,
                            bounds,
                        )
                    })
                    .and(|| {
                        self.match_row_dir(head_vars, &x.errors, &y.errors, Dir::Sub, false, bounds)
                    })
            }
            (Dir::Sup, Some(x), Some(y)) => {
                if s2 == Stage::Runtime && s1 == Stage::Sync {
                    return Match::No;
                }
                let pat_has_row_var = self
                    .store
                    .normalize(&x.services)
                    .tails
                    .iter()
                    .any(|t| matches!(t, Type::Var(v) if is_head_var(head_vars, v)));
                let live_ok = if pat_has_row_var {
                    Match::Yes
                } else {
                    let mut y_live = live_caps_in_row(self, &y.services);
                    merge_norm(&mut y_live, t_caps);
                    let mut x_live = live_caps_in_row(self, &x.services);
                    merge_norm(&mut x_live, p_caps);
                    norm_within(&y_live, &x_live)
                };
                live_ok
                    .and(|| {
                        self.match_row_dir(
                            head_vars,
                            &x.services,
                            &y.services,
                            Dir::Sup,
                            true,
                            bounds,
                        )
                    })
                    .and(|| {
                        self.match_row_dir(head_vars, &x.errors, &y.errors, Dir::Sup, false, bounds)
                    })
            }
            _ => Match::No,
        }
    }

    fn match_row_dir(
        &mut self,
        head_vars: &[Binder],
        pat: &Type,
        target: &Type,
        dir: Dir,
        is_services: bool,
        bounds: &mut VarBounds,
    ) -> Match {
        let p_norm = if is_services {
            row_without_live_norm(self, pat)
        } else {
            stage_norm(self, pat)
        };
        let t_norm = if is_services {
            row_without_live_norm(self, target)
        } else {
            stage_norm(self, target)
        };
        let t_open = norm_has_meta_tail(&t_norm);
        let p_head_var: Option<String> = p_norm.tails.iter().find_map(|t| match t {
            Type::Var(v) if is_head_var(head_vars, v) => Some(v.clone()),
            _ => None,
        });
        match dir {
            Dir::Eq => self.match_row_eq(head_vars, &p_norm, &t_norm, t_open, p_head_var, bounds),
            Dir::Sub => self.match_row_sub(head_vars, &p_norm, &t_norm, t_open, p_head_var, bounds),
            Dir::Sup => self.match_row_sup(head_vars, &p_norm, &t_norm, t_open, p_head_var, bounds),
        }
    }

    fn match_row_eq(
        &mut self,
        head_vars: &[Binder],
        p_norm: &Norm,
        t_norm: &Norm,
        t_open: bool,
        p_head_var: Option<String>,
        bounds: &mut VarBounds,
    ) -> Match {
        let mut res = Match::Yes;
        for (l, p_ty) in &p_norm.entries {
            match t_norm.entries.iter().find(|(tl, _)| tl == l) {
                Some((_, t_ty)) => {
                    res = res.and(|| self.collect_match(head_vars, p_ty, t_ty, Dir::Eq, bounds));
                }
                None if t_open => res = res.and(|| Match::Unknown),
                None => return Match::No,
            }
        }
        let rest: Vec<(Elem, Type)> = t_norm
            .entries
            .iter()
            .filter(|(l, _)| !p_norm.entries.iter().any(|(pl, _)| pl == l))
            .cloned()
            .collect();
        if let Some(v) = p_head_var {
            if t_open && !rest.is_empty() {
                return Match::Unknown;
            }
            let rest_row = Norm {
                entries: rest,
                tails: t_norm.tails.clone(),
            }
            .into_type();
            if let Some(prev) = bounds.rows.get(&v) {
                res = res.and(|| self.match_rigid_row_eq(prev, &rest_row));
            } else {
                bounds.rows.insert(v, rest_row);
            }
        } else if !rest.is_empty() {
            return Match::No;
        } else if p_norm.tails != t_norm.tails {
            if t_open {
                res = res.and(|| Match::Unknown);
            } else {
                return Match::No;
            }
        }
        res
    }

    fn match_row_sub(
        &mut self,
        head_vars: &[Binder],
        p_norm: &Norm,
        t_norm: &Norm,
        t_open: bool,
        p_head_var: Option<String>,
        bounds: &mut VarBounds,
    ) -> Match {
        let mut res = Match::Yes;
        for (l, p_ty) in &p_norm.entries {
            match t_norm.entries.iter().find(|(tl, _)| tl == l) {
                Some((_, t_ty)) => {
                    res = res.and(|| self.collect_match(head_vars, p_ty, t_ty, Dir::Eq, bounds));
                }
                None if t_open => res = res.and(|| Match::Unknown),
                None => return Match::No,
            }
        }
        if let Some(v) = p_head_var {
            if t_open {
                return Match::Unknown;
            }
            bounds.rows.entry(v).or_insert_with(Type::empty_row);
        } else {
            for pt in &p_norm.tails {
                if !t_norm.tails.contains(pt) {
                    if t_open {
                        res = res.and(|| Match::Unknown);
                    } else {
                        return Match::No;
                    }
                }
            }
        }
        res
    }

    fn match_row_sup(
        &mut self,
        head_vars: &[Binder],
        p_norm: &Norm,
        t_norm: &Norm,
        t_open: bool,
        p_head_var: Option<String>,
        bounds: &mut VarBounds,
    ) -> Match {
        let mut res = Match::Yes;
        let mut missing_entries = Vec::new();
        for (l, t_ty) in &t_norm.entries {
            match p_norm.entries.iter().find(|(pl, _)| pl == l) {
                Some((_, p_ty)) => {
                    res = res.and(|| self.collect_match(head_vars, p_ty, t_ty, Dir::Eq, bounds));
                }
                None if p_head_var.is_some() => missing_entries.push((l.clone(), t_ty.clone())),
                None => return Match::No,
            }
        }
        if t_open {
            return Match::Unknown;
        }
        let mut missing_tails = Vec::new();
        for tt in &t_norm.tails {
            if !p_norm.tails.contains(tt) {
                if p_head_var.is_some() {
                    missing_tails.push(tt.clone());
                } else {
                    return Match::No;
                }
            }
        }
        if let Some(v) = p_head_var {
            let needed = Norm {
                entries: missing_entries,
                tails: missing_tails,
            }
            .into_type();
            bounds
                .rows
                .entry(v)
                .and_modify(|existing| {
                    *existing = self
                        .store
                        .normalize(&Type::Union(
                            Box::new(existing.clone()),
                            Box::new(needed.clone()),
                        ))
                        .into_type();
                })
                .or_insert(needed);
        }
        res
    }
}
