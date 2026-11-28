//! Bound resolution, directed subtype verification, and type lattice
//! (`join_ty` / `meet_ty`) operations for B1 matching and K1' witnesses.

use super::VarBounds;
use super::caps::{
    intersect_norm, live_caps_in_row, merge_norm, norm_has_meta_tail, norm_within, reach_within,
    row_without_live_norm, split_caps_cow, stage_norm, wrap_caps,
};
use crate::types::Checker;
use crate::types::classes::matching::{Match, Subst};
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::{Binder, Effects, Norm, Row, Stage, Type, Written, stage_of};
use crate::types::variance::Variance;

impl Checker<'_> {
    pub(super) fn solve_var_bounds(
        &mut self,
        head_vars: &[Binder],
        mut bounds: VarBounds,
    ) -> Option<Subst> {
        let mut subst = Subst::new();
        for b in head_vars {
            let v = &b.name;
            let chosen = match &b.kind {
                Kind::ServiceRow | Kind::ErrorRow => bounds
                    .rows
                    .remove(v)
                    .or_else(|| bounds.exact.remove(v))
                    .unwrap_or_else(Type::empty_row),
                Kind::CaptureRow => bounds
                    .caps
                    .remove(v)
                    .map(Norm::into_type)
                    .or_else(|| bounds.exact.remove(v))
                    .unwrap_or_else(Type::empty_row),
                _ => {
                    if let Some(t) = bounds.exact.remove(v) {
                        t
                    } else if let Some(lows) = bounds.lower.remove(v) {
                        let mut iter = lows.into_iter();
                        let mut acc = iter.next()?;
                        for t in iter {
                            acc = self.join_ty(&acc, &t)?;
                        }
                        acc
                    } else if let Some(highs) = bounds.upper.remove(v) {
                        let mut iter = highs.into_iter();
                        let mut acc = iter.next()?;
                        for t in iter {
                            acc = self.meet_ty(&acc, &t)?;
                        }
                        acc
                    } else {
                        continue;
                    }
                }
            };
            subst.push((v.clone(), chosen));
        }
        Some(subst)
    }

    pub(super) fn verify_ty_le(&mut self, sub: &Type, sup: &Type) -> Match {
        if matches!(sub, Type::Error) || matches!(sup, Type::Error) {
            return Match::Yes;
        }
        if sub == sup {
            return Match::Yes;
        }
        let reach_m = reach_within(self, sub, sup);
        if reach_m == Match::No {
            return Match::No;
        }
        let (ba, ca) = split_caps_cow(self, sub);
        let (bb, cb) = split_caps_cow(self, sup);
        if matches!(&*ba, Type::Meta(_)) || matches!(&*bb, Type::Meta(_)) {
            return Match::Unknown;
        }
        let body_m = match (&*ba, &*bb) {
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
                let stage_m = self.verify_stage_le(e1.as_deref(), &ca, e2.as_deref(), &cb);
                stage_m
                    .and(|| self.verify_ty_le(p2, p1))
                    .and(|| self.verify_ty_le(r1, r2))
            }
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys)
                .fold(Match::Yes, |m, (x, y)| m.and(|| self.verify_ty_le(x, y))),
            (Type::Record(r1), Type::Record(r2))
                if r1.tail.is_none()
                    && r2.tail.is_none()
                    && r1.entries.len() == r2.entries.len()
                    && r1.entries.iter().all(|(l, _)| r2.get(l).is_some()) =>
            {
                r1.entries.iter().fold(Match::Yes, |m, (l, x)| {
                    m.and(|| r2.get(l).map_or(Match::No, |y| self.verify_ty_le(x, y)))
                })
            }
            (Type::App { .. }, Type::App { .. }) => self.verify_app_le(&ba, &bb),
            _ => self.match_rigid_eq(&ba, &bb),
        };
        reach_m.and(|| body_m)
    }

    fn verify_stage_le(
        &self,
        e1: Option<&Effects>,
        ca: &Norm,
        e2: Option<&Effects>,
        cb: &Norm,
    ) -> Match {
        match (e1, e2) {
            (None, _) => Match::Yes,
            (Some(_), None) => Match::No,
            (Some(x), Some(y)) => {
                if stage_of(Some(x)) == Stage::Runtime && stage_of(Some(y)) == Stage::Sync {
                    return Match::No;
                }
                let mut live_x = live_caps_in_row(self, &x.services);
                merge_norm(&mut live_x, ca);
                let mut live_y = live_caps_in_row(self, &y.services);
                merge_norm(&mut live_y, cb);
                norm_within(&live_x, &live_y)
                    .and(|| self.verify_row_le(&x.services, &y.services, true))
                    .and(|| self.verify_row_le(&x.errors, &y.errors, false))
            }
        }
    }

    fn verify_row_le(&self, sub: &Type, sup: &Type, is_services: bool) -> Match {
        let s_norm = if is_services {
            row_without_live_norm(self, sub)
        } else {
            stage_norm(self, sub)
        };
        let t_norm = if is_services {
            row_without_live_norm(self, sup)
        } else {
            stage_norm(self, sup)
        };
        let t_open = norm_has_meta_tail(&t_norm);
        let mut res = Match::Yes;
        for (l, a_ty) in &s_norm.entries {
            match t_norm.entries.iter().find(|(tl, _)| tl == l) {
                Some((_, b_ty)) => res = res.and(|| self.match_rigid_eq(a_ty, b_ty)),
                None if t_open => res = res.and(|| Match::Unknown),
                None => return Match::No,
            }
        }
        for tail in &s_norm.tails {
            if !t_norm.tails.contains(tail) {
                if matches!(tail, Type::Meta(_)) || t_open {
                    res = res.and(|| Match::Unknown);
                } else {
                    return Match::No;
                }
            }
        }
        res
    }

    fn verify_app_le(&mut self, a: &Type, b: &Type) -> Match {
        let (h1, args1) = a.spine();
        let (h2, args2) = b.spine();
        if args1.len() != args2.len() {
            return Match::No;
        }
        if h1.is_lang(Lang::Eff)
            && h2.is_lang(Lang::Eff)
            && let ([s1, e1, r1], [s2, e2, r2]) = (&args1[..], &args2[..])
        {
            return self
                .verify_row_le(s1, s2, true)
                .and(|| self.verify_row_le(e1, e2, false))
                .and(|| self.verify_ty_le(r1, r2));
        }
        if h1.is_lang(Lang::Layer)
            && h2.is_lang(Lang::Layer)
            && let ([i1, e1, o1], [i2, e2, o2]) = (&args1[..], &args2[..])
        {
            return self
                .verify_row_le(i1, i2, true)
                .and(|| self.verify_row_le(e1, e2, false))
                .and(|| self.match_rigid_eq(o1, o2));
        }
        let (
            Type::Con {
                module: m1,
                name: n1,
            },
            Type::Con {
                module: m2,
                name: n2,
            },
        ) = (h1, h2)
        else {
            return self.match_rigid_eq(a, b);
        };
        if m1 != m2 || n1 != n2 {
            return Match::No;
        }
        let Some(signs) = self.app_variances(m1, n1, args1.len()) else {
            return self.match_rigid_eq(a, b);
        };
        let is_dict = n1.ends_with("$Dict");
        args1
            .iter()
            .zip(&args2)
            .zip(&signs)
            .fold(Match::Yes, |m, ((x, y), sign)| {
                m.and(|| {
                    let arg_m = match sign {
                        Variance::Covariant => self.verify_ty_le(x, y),
                        Variance::Contravariant => {
                            self.verify_ty_le(y, x).and(|| reach_within(self, x, y))
                        }
                        Variance::Invariant | Variance::Bivariant => self.match_rigid_eq(x, y),
                    };
                    if is_dict {
                        arg_m
                    } else {
                        reach_within(self, x, y).and(|| arg_m)
                    }
                })
            })
    }

    pub(super) fn join_ty(&mut self, a: &Type, b: &Type) -> Option<Type> {
        if self.verify_ty_le(a, b) == Match::Yes {
            return Some(b.clone());
        }
        if self.verify_ty_le(b, a) == Match::Yes {
            return Some(a.clone());
        }
        let (ba, ca) = split_caps_cow(self, a);
        let (bb, cb) = split_caps_cow(self, b);
        let mut caps = ca;
        merge_norm(&mut caps, &cb);
        let inner = match (&*ba, &*bb) {
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
            ) => Type::Fun {
                param: Box::new(self.meet_ty(p1, p2)?),
                effects: self.join_effects(e1.as_deref(), e2.as_deref()).ok()?,
                result: Box::new(self.join_ty(r1, r2)?),
            },
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
                let mut out = Vec::with_capacity(xs.len());
                for (x, y) in xs.iter().zip(ys) {
                    out.push(self.join_ty(x, y)?);
                }
                Type::Tuple(out)
            }
            (Type::Record(r1), Type::Record(r2))
                if r1.tail.is_none()
                    && r2.tail.is_none()
                    && r1.entries.len() == r2.entries.len()
                    && r1.entries.iter().all(|(l, _)| r2.get(l).is_some()) =>
            {
                let mut entries = Vec::with_capacity(r1.entries.len());
                for (l, x) in &r1.entries {
                    let y = r2.get(l)?;
                    entries.push((l.clone(), self.join_ty(x, y)?));
                }
                Type::Record(Row::closed(entries))
            }
            (Type::App { .. }, Type::App { .. }) => {
                let ((h1, xs), (h2, ys)) = (ba.spine(), bb.spine());
                let (
                    Type::Con {
                        module: m1,
                        name: n1,
                    },
                    Type::Con {
                        module: m2,
                        name: n2,
                    },
                ) = (h1, h2)
                else {
                    return None;
                };
                if m1 != m2 || n1 != n2 || xs.len() != ys.len() {
                    return None;
                }
                let signs = self.app_variances(m1, n1, xs.len())?;
                let mut out = Type::con(m1, n1);
                for ((x, y), sign) in xs.iter().zip(&ys).zip(&signs) {
                    let arg = match sign {
                        Variance::Covariant => self.join_ty(x, y)?,
                        Variance::Contravariant => self.meet_ty(x, y)?,
                        Variance::Invariant | Variance::Bivariant if x == y => (*x).clone(),
                        _ => return None,
                    };
                    out = Type::app(out, arg);
                }
                out
            }
            _ => return None,
        };
        Some(wrap_caps(inner, caps))
    }

    pub(super) fn meet_ty(&mut self, a: &Type, b: &Type) -> Option<Type> {
        if self.verify_ty_le(a, b) == Match::Yes {
            return Some(a.clone());
        }
        if self.verify_ty_le(b, a) == Match::Yes {
            return Some(b.clone());
        }
        let (ba, ca) = split_caps_cow(self, a);
        let (bb, cb) = split_caps_cow(self, b);
        let caps = intersect_norm(&ca, &cb);
        let inner = match (&*ba, &*bb) {
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
            ) => Type::Fun {
                param: Box::new(self.join_ty(p1, p2)?),
                effects: self.meet_effects(e1.as_deref(), e2.as_deref()),
                result: Box::new(self.meet_ty(r1, r2)?),
            },
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
                let mut out = Vec::with_capacity(xs.len());
                for (x, y) in xs.iter().zip(ys) {
                    out.push(self.meet_ty(x, y)?);
                }
                Type::Tuple(out)
            }
            (Type::Record(r1), Type::Record(r2))
                if r1.tail.is_none()
                    && r2.tail.is_none()
                    && r1.entries.len() == r2.entries.len()
                    && r1.entries.iter().all(|(l, _)| r2.get(l).is_some()) =>
            {
                let mut entries = Vec::with_capacity(r1.entries.len());
                for (l, x) in &r1.entries {
                    let y = r2.get(l)?;
                    entries.push((l.clone(), self.meet_ty(x, y)?));
                }
                Type::Record(Row::closed(entries))
            }
            (Type::App { .. }, Type::App { .. }) => {
                let ((h1, xs), (h2, ys)) = (ba.spine(), bb.spine());
                let (
                    Type::Con {
                        module: m1,
                        name: n1,
                    },
                    Type::Con {
                        module: m2,
                        name: n2,
                    },
                ) = (h1, h2)
                else {
                    return None;
                };
                if m1 != m2 || n1 != n2 || xs.len() != ys.len() {
                    return None;
                }
                let signs = self.app_variances(m1, n1, xs.len())?;
                let mut out = Type::con(m1, n1);
                for ((x, y), sign) in xs.iter().zip(&ys).zip(&signs) {
                    let arg = match sign {
                        Variance::Covariant => self.meet_ty(x, y)?,
                        Variance::Contravariant => self.join_ty(x, y)?,
                        Variance::Invariant | Variance::Bivariant if x == y => (*x).clone(),
                        _ => return None,
                    };
                    out = Type::app(out, arg);
                }
                out
            }
            _ => return None,
        };
        Some(wrap_caps(inner, caps))
    }

    fn join_effects(
        &self,
        e1: Option<&Effects>,
        e2: Option<&Effects>,
    ) -> Result<Option<Box<Effects>>, ()> {
        match (e1, e2) {
            (None, None) => Ok(None),
            (None, Some(e)) | (Some(e), None) => Ok(Some(Box::new(e.clone()))),
            (Some(a), Some(b)) => {
                let services = self.join_rows(&a.services, &b.services).ok_or(())?;
                let errors = self.join_rows(&a.errors, &b.errors).ok_or(())?;
                Ok(Some(Box::new(Effects {
                    services,
                    errors,
                    sync: a.sync && b.sync,
                    direct: a.direct && b.direct,
                    provides: None,
                    written: Written::default(),
                })))
            }
        }
    }

    fn join_rows(&self, r1: &Type, r2: &Type) -> Option<Type> {
        let mut n1 = row_without_live_norm(self, r1);
        let n2 = row_without_live_norm(self, r2);
        for (l, ty) in n2.entries {
            if let Some((_, existing)) = n1.entries.iter().find(|(el, _)| *el == l) {
                if existing != &ty {
                    return None;
                }
            } else {
                n1.entries.push((l, ty));
            }
        }
        for tail in n2.tails {
            if !n1.tails.contains(&tail) {
                n1.tails.push(tail);
            }
        }
        Row::sort_entries(&mut n1.entries);
        Some(n1.into_type())
    }

    fn meet_effects(&self, e1: Option<&Effects>, e2: Option<&Effects>) -> Option<Box<Effects>> {
        match (e1, e2) {
            (None, _) | (_, None) => None,
            (Some(a), Some(b)) => Some(Box::new(Effects {
                services: intersect_norm(
                    &row_without_live_norm(self, &a.services),
                    &row_without_live_norm(self, &b.services),
                )
                .into_type(),
                errors: intersect_norm(&stage_norm(self, &a.errors), &stage_norm(self, &b.errors))
                    .into_type(),
                sync: a.sync || b.sync,
                direct: a.direct || b.direct,
                provides: None,
                written: Written::default(),
            })),
        }
    }
}
