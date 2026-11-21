//! Capture-row, reach-set, and stage-row normalization helpers for variance
//! matching and K1' overlap checks.

use std::borrow::Cow;
use std::collections::HashSet;
use std::ptr;

use super::{Dir, VarBounds};
use crate::types::Checker;
use crate::types::classes::matching::Match;
use crate::types::elem::{Atom, Elem};
use crate::types::lang::{self, Lang};
use crate::types::ty::{Binder, Elided, Norm, Row, Type};

pub(super) fn is_head_var(head_vars: &[Binder], name: &str) -> bool {
    head_vars.iter().any(|b| b.name == name)
}

pub(super) fn norm_is_empty(norm: &Norm) -> bool {
    norm.entries.is_empty() && norm.tails.is_empty()
}

pub(super) fn norm_has_meta_tail(norm: &Norm) -> bool {
    norm.tails.iter().any(|t| matches!(t, Type::Meta(_)))
}

pub(super) fn merge_norm(into: &mut Norm, other: &Norm) {
    into.entries.extend(other.entries.iter().cloned());
    Row::sort_entries(&mut into.entries);
    into.entries.dedup_by(|a, b| a.0 == b.0);
    for tail in &other.tails {
        if !into.tails.contains(tail) {
            into.tails.push(tail.clone());
        }
    }
}

pub(super) fn split_caps_cow<'t>(checker: &Checker<'_>, ty: &'t Type) -> (Cow<'t, Type>, Norm) {
    if !matches!(
        ty,
        Type::Meta(_) | Type::Captured { .. } | Type::Bounded { .. }
    ) && matches!(
        checker.borrowed_witness_head(ty),
        Cow::Borrowed(r) if ptr::eq(r, ty)
    ) {
        return (Cow::Borrowed(ty), Norm::default());
    }
    let (bare, caps) = split_caps(checker, ty);
    (Cow::Owned(bare), caps)
}

pub(super) fn split_caps(checker: &Checker<'_>, ty: &Type) -> (Type, Norm) {
    let mut cur = checker.witness_head(ty);
    let mut caps = Norm::default();
    for _ in 0..64 {
        match cur {
            Type::Captured {
                ty: inner,
                captures,
                ..
            }
            | Type::Bounded {
                ty: inner,
                captures,
            } => {
                let norm = checker.store.normalize(&checker.expanded(&captures));
                merge_norm(&mut caps, &norm);
                cur = checker.witness_head(&inner);
            }
            _ => break,
        }
    }
    (cur, caps)
}

pub(super) fn stage_norm(checker: &Checker<'_>, row: &Type) -> Norm {
    let mut norm = checker.store.normalize(row);
    norm.entries.retain(|(label, ty)| {
        !matches!(label, Elem::Live) || !norm_is_empty(&checker.store.normalize(ty))
    });
    norm
}

pub(super) fn live_caps_in_row(checker: &Checker<'_>, row: &Type) -> Norm {
    let mut out = Norm::default();
    let norm = checker.store.normalize(row);
    for (label, ty) in &norm.entries {
        if matches!(label, Elem::Live) {
            let live = checker.store.normalize(ty);
            merge_norm(&mut out, &live);
        } else if let Elem::Named(name) = label
            && lang::label(Lang::Scope) == Some(name.as_str())
            && let (_, args) = ty.spine()
            && let [region] = args.as_slice()
            && let Some(atom) = Atom::of(region)
        {
            out.entries.push((Elem::Capture(atom), (*region).clone()));
        }
    }
    Row::sort_entries(&mut out.entries);
    out.entries.dedup_by(|a, b| a.0 == b.0);
    out
}

pub(super) fn reach_of(checker: &Checker<'_>, ty: &Type) -> Norm {
    let mut out = checker.captures_of(ty);
    ty.visit(&mut |t| {
        if let Type::Fun {
            effects: Some(e), ..
        } = t
        {
            let live = live_caps_in_row(checker, &e.services);
            merge_norm(&mut out, &live);
        } else if let Type::App { .. } = t
            && let (head, args) = t.spine()
            && let [services, _, _] = args.as_slice()
            && head.is_lang(Lang::Eff)
        {
            let live = live_caps_in_row(checker, services);
            merge_norm(&mut out, &live);
        }
    });
    Row::sort_entries(&mut out.entries);
    out.entries.dedup_by(|a, b| a.0 == b.0);
    out
}

pub(super) fn norm_within(sub: &Norm, sup: &Norm) -> Match {
    let sup_meta = norm_has_meta_tail(sup);
    let mut result = Match::Yes;
    for (label, _) in &sub.entries {
        if sup.entries.iter().any(|(l, _)| l == label) {
            continue;
        }
        if let Elem::Capture(Atom::Meta(_)) = label {
            result = result.and(|| Match::Unknown);
        } else if sup_meta {
            result = result.and(|| Match::Unknown);
        } else {
            return Match::No;
        }
    }
    for tail in &sub.tails {
        if sup.tails.contains(tail) {
            continue;
        }
        if matches!(tail, Type::Meta(_)) || sup_meta {
            result = result.and(|| Match::Unknown);
        } else {
            return Match::No;
        }
    }
    result
}

pub(super) fn reach_within(checker: &Checker<'_>, sub: &Type, sup: &Type) -> Match {
    if sub == sup {
        return Match::Yes;
    }
    let lower = reach_of(checker, sub);
    let upper = reach_of(checker, sup);
    norm_within(&lower, &upper)
}

pub(super) fn row_without_live_norm(checker: &Checker<'_>, row: &Type) -> Norm {
    let mut norm = checker.store.normalize(row);
    norm.entries
        .retain(|(label, _)| !matches!(label, Elem::Live));
    norm
}

pub(super) fn has_type_var_in(ty: &Type, vars: &HashSet<String>) -> bool {
    ty.contains(&mut |t| match t {
        Type::Var(v) => vars.contains(v),
        Type::Record(r) | Type::Row(r) => r.entries.iter().any(|(l, _)| match l {
            Elem::Capture(Atom::Var(n))
            | Elem::Error(Atom::Var(n))
            | Elem::Service(Atom::Var(n)) => vars.contains(n),
            _ => false,
        }),
        _ => false,
    })
}

pub(super) fn match_caps_dir(
    head_vars: &[Binder],
    p_caps: &Norm,
    t_caps: &Norm,
    dir: Dir,
    bounds: &mut VarBounds,
) -> Match {
    let head_cap_vars: Vec<String> = p_caps
        .tails
        .iter()
        .filter_map(|t| match t {
            Type::Var(v) if is_head_var(head_vars, v) => Some(v.clone()),
            _ => None,
        })
        .collect();
    if head_cap_vars.is_empty() {
        return match dir {
            Dir::Eq => norm_within(p_caps, t_caps).and(|| norm_within(t_caps, p_caps)),
            Dir::Sub => norm_within(p_caps, t_caps),
            Dir::Sup => norm_within(t_caps, p_caps),
        };
    }
    // A head capture variable cannot safely bind to an unsolved capture meta tail.
    if norm_has_meta_tail(t_caps) {
        return Match::Unknown;
    }
    if head_cap_vars.len() == 1 {
        let v = &head_cap_vars[0];
        let mut rem = t_caps.clone();
        rem.entries
            .retain(|(l, _)| !p_caps.entries.iter().any(|(pl, _)| pl == l));
        rem.tails.retain(|t| !p_caps.tails.iter().any(|pt| pt == t));
        bounds
            .caps
            .entry(v.clone())
            .and_modify(|c| merge_norm(c, &rem))
            .or_insert(rem);
    }
    Match::Yes
}

pub(super) fn intersect_norm(a: &Norm, b: &Norm) -> Norm {
    Norm {
        entries: a
            .entries
            .iter()
            .filter(|(l, t)| b.entries.iter().any(|(bl, bt)| bl == l && bt == t))
            .cloned()
            .collect(),
        tails: a
            .tails
            .iter()
            .filter(|t| b.tails.contains(t))
            .cloned()
            .collect(),
    }
}

pub(super) fn wrap_caps(inner: Type, caps: Norm) -> Type {
    if norm_is_empty(&caps) {
        inner
    } else {
        Type::Captured {
            ty: Box::new(inner),
            captures: Box::new(caps.into_type()),
            value: false,
            elided: Elided::default(),
        }
    }
}
