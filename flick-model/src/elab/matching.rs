//! Direction-aware head matching (`collect_match`, `join_ty`, `meet_ty`,
//! `match_instance`) for B1 instance selection.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::sub::{caps_sub, live_service, split_caps};
use super::{Constraint, InstanceDecl, SubstVal, TypeSubst, class_sign, data_sign, serves, ty_le};
use crate::decls::Decls;
use crate::syntax::Name;
use crate::types::{Caps, Fields, Kind, Row, Stage, Type, Var};
use crate::variance::{Sign, spine};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Eq,
    /// `pattern[σ] ≤ target`
    Sub,
    /// `target ≤ pattern[σ]`
    Sup,
}

impl Dir {
    fn flip(self) -> Self {
        match self {
            Dir::Eq => Dir::Eq,
            Dir::Sub => Dir::Sup,
            Dir::Sup => Dir::Sub,
        }
    }
}

#[derive(Default)]
struct VarBounds {
    exact: HashMap<Name, Type>,
    lower: HashMap<Name, Vec<Type>>,
    upper: HashMap<Name, Vec<Type>>,
    rows: HashMap<Name, Row>,
    caps: HashMap<Name, Caps>,
}

fn collect_match(
    decls: &Decls,
    head_vars: &HashSet<Name>,
    pat: &Type,
    target: &Type,
    dir: Dir,
    bounds: &mut VarBounds,
) -> bool {
    if let Type::Var(Var::Named(n)) = pat
        && head_vars.contains(n)
    {
        match dir {
            Dir::Eq => {
                if let Some(prev) = bounds.exact.get(n) {
                    return prev == target;
                }
                bounds.exact.insert(n.clone(), target.clone());
            }
            Dir::Sub => bounds
                .upper
                .entry(n.clone())
                .or_default()
                .push(target.clone()),
            Dir::Sup => bounds
                .lower
                .entry(n.clone())
                .or_default()
                .push(target.clone()),
        }
        return true;
    }

    let (p_bare, p_caps) = split_caps(pat);
    let (t_bare, t_caps) = split_caps(target);
    if !match_caps(head_vars, &p_caps, &t_caps, dir, bounds) {
        return false;
    }

    match (p_bare, t_bare) {
        (Type::Fun(p1, s1, r1), Type::Fun(p2, s2, r2)) => {
            collect_match(decls, head_vars, p1, p2, dir.flip(), bounds)
                && match_stage(decls, head_vars, s1, s2, dir, bounds)
                && collect_match(decls, head_vars, r1, r2, dir, bounds)
        }
        (Type::Eff(r1, e1, a1), Type::Eff(r2, e2, a2)) => {
            match_row(decls, head_vars, r1, r2, dir, bounds)
                && match_row(decls, head_vars, e1, e2, dir, bounds)
                && collect_match(decls, head_vars, a1, a2, dir, bounds)
        }
        (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => xs
            .iter()
            .zip(ys.iter())
            .all(|(x, y)| collect_match(decls, head_vars, x, y, dir, bounds)),
        (Type::Record(f), Type::Record(g))
            if f.tail.is_none() && g.tail.is_none() && f.fields.keys().eq(g.fields.keys()) =>
        {
            f.fields
                .values()
                .zip(g.fields.values())
                .all(|(x, y)| collect_match(decls, head_vars, x, y, dir, bounds))
        }
        (Type::App(..), Type::App(..)) => {
            let (Some((c1, xs)), Some((c2, ys))) = (spine(p_bare), spine(t_bare)) else {
                return p_bare == t_bare;
            };
            if c1 != c2 || xs.len() != ys.len() {
                return false;
            }
            xs.iter().zip(ys.iter()).enumerate().all(|(j, (x, y))| {
                match (dir, data_sign(decls, c1, j)) {
                    (Dir::Eq, _) | (_, Sign::Inv | Sign::Bi) => {
                        collect_match(decls, head_vars, x, y, Dir::Eq, bounds)
                    }
                    (_, Sign::Pos) => collect_match(decls, head_vars, x, y, dir, bounds),
                    (_, Sign::Neg) => collect_match(decls, head_vars, x, y, dir.flip(), bounds),
                }
            })
        }
        (Type::Dict(c1, xs), Type::Dict(c2, ys)) if c1 == c2 && xs.len() == ys.len() => {
            xs.iter().zip(ys.iter()).enumerate().all(|(j, (x, y))| {
                match (dir, class_sign(decls, c1, j)) {
                    (Dir::Eq, _) | (_, Sign::Inv | Sign::Bi) => {
                        collect_match(decls, head_vars, x, y, Dir::Eq, bounds)
                    }
                    (_, Sign::Pos) => collect_match(decls, head_vars, x, y, dir, bounds),
                    (_, Sign::Neg) => collect_match(decls, head_vars, x, y, dir.flip(), bounds),
                }
            })
        }
        (Type::Ref(x), Type::Ref(y)) => collect_match(decls, head_vars, x, y, Dir::Eq, bounds),
        (Type::Errors(r1), Type::Errors(r2))
        | (Type::Impl(r1), Type::Impl(r2))
        | (Type::Tag(r1), Type::Tag(r2))
        | (Type::Env(r1), Type::Env(r2)) => match_row(decls, head_vars, r1, r2, Dir::Eq, bounds),
        _ => p_bare == t_bare,
    }
}

fn match_caps(
    head_vars: &HashSet<Name>,
    pat: &Caps,
    target: &Caps,
    dir: Dir,
    bounds: &mut VarBounds,
) -> bool {
    let head_cap_vars: Vec<&Name> = pat
        .vars
        .iter()
        .filter_map(|v| match v {
            Var::Named(n) if head_vars.contains(n) => Some(n),
            _ => None,
        })
        .collect();
    if head_cap_vars.is_empty() {
        return match dir {
            Dir::Eq => pat == target,
            Dir::Sub => caps_sub(pat, target),
            Dir::Sup => caps_sub(target, pat),
        };
    }
    if head_cap_vars.len() == 1 {
        let v = head_cap_vars[0];
        let mut remainder = target.clone();
        for a in &pat.atoms {
            remainder.atoms.remove(a);
        }
        for w in &pat.vars {
            if !matches!(w, Var::Named(n) if n == v) {
                remainder.vars.remove(w);
            }
        }
        bounds
            .caps
            .entry(v.clone())
            .and_modify(|c| *c = c.union(&remainder))
            .or_insert(remainder);
    }
    true
}

fn match_row(
    decls: &Decls,
    head_vars: &HashSet<Name>,
    pat: &Row,
    target: &Row,
    dir: Dir,
    bounds: &mut VarBounds,
) -> bool {
    let head_row_vars: Vec<&Name> = pat
        .vars
        .iter()
        .filter_map(|v| match v {
            Var::Named(n) if head_vars.contains(n) => Some(n),
            _ => None,
        })
        .collect();
    for (svc, p_args) in &pat.elems {
        if *svc == live_service() {
            continue;
        }
        if let Some(t_args) = target.elems.get(svc) {
            if p_args.len() != t_args.len()
                || !p_args
                    .iter()
                    .zip(t_args.iter())
                    .all(|(x, y)| collect_match(decls, head_vars, x, y, Dir::Eq, bounds))
            {
                return false;
            }
        } else if dir != Dir::Sup {
            return false;
        }
    }
    if head_row_vars.len() == 1 {
        let v = head_row_vars[0];
        let mut rem = target.clone();
        for svc in pat.elems.keys() {
            rem.elems.remove(svc);
        }
        bounds
            .rows
            .entry(v.clone())
            .and_modify(|r| {
                if let Some(s) = r.sum(&rem) {
                    *r = s;
                }
            })
            .or_insert(rem);
    }
    true
}

fn match_stage(
    decls: &Decls,
    head_vars: &HashSet<Name>,
    pat: &Stage,
    target: &Stage,
    dir: Dir,
    bounds: &mut VarBounds,
) -> bool {
    match (dir, pat, target) {
        (Dir::Eq, Stage::Pure, Stage::Pure)
        | (Dir::Sub, Stage::Pure, _)
        | (Dir::Sup, _, Stage::Pure) => true,
        (Dir::Eq, Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => {
            match_row(decls, head_vars, r1, r2, Dir::Eq, bounds)
                && match_row(decls, head_vars, e1, e2, Dir::Eq, bounds)
        }
        (Dir::Sub, Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => {
            match_row(decls, head_vars, r1, r2, Dir::Sub, bounds)
                && match_row(decls, head_vars, e1, e2, Dir::Sub, bounds)
        }
        (Dir::Sup, Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => {
            match_row(decls, head_vars, r1, r2, Dir::Sup, bounds)
                && match_row(decls, head_vars, e1, e2, Dir::Sup, bounds)
        }
        _ => false,
    }
}

/// Least upper bound of two types (used when a head variable collects multiple
/// lower bounds under contravariant matching).
fn join_ty(decls: &Decls, a: &Type, b: &Type) -> Option<Type> {
    if ty_le(decls, a, b) {
        return Some(b.clone());
    }
    if ty_le(decls, b, a) {
        return Some(a.clone());
    }
    let (ba, ca) = split_caps(a);
    let (bb, cb) = split_caps(b);
    let caps = ca.union(&cb);
    let inner = match (ba, bb) {
        (Type::Fun(p1, s1, r1), Type::Fun(p2, s2, r2)) => Type::fun(
            meet_ty(decls, p1, p2)?,
            s1.join(s2)?,
            join_ty(decls, r1, r2)?,
        ),
        (Type::Eff(r1, e1, t1), Type::Eff(r2, e2, t2)) => {
            Type::Eff(r1.sum(r2)?, e1.sum(e2)?, Rc::new(join_ty(decls, t1, t2)?))
        }
        (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
            let joined: Option<Vec<Type>> = xs
                .iter()
                .zip(ys.iter())
                .map(|(x, y)| join_ty(decls, x, y))
                .collect();
            Type::Tuple(Rc::from(joined?))
        }
        (Type::Record(f), Type::Record(g))
            if f.tail.is_none() && g.tail.is_none() && f.fields.keys().eq(g.fields.keys()) =>
        {
            let mut fields = BTreeMap::new();
            for (k, x) in &f.fields {
                fields.insert(k.clone(), join_ty(decls, x, &g.fields[k])?);
            }
            Type::Record(Fields { fields, tail: None })
        }
        (Type::App(..), Type::App(..)) => {
            let (Some((p1, xs)), Some((p2, ys))) = (spine(ba), spine(bb)) else {
                return None;
            };
            if p1 != p2 || xs.len() != ys.len() {
                return None;
            }
            let mut out = Type::Con(p1.clone());
            for (j, (x, y)) in xs.iter().zip(ys.iter()).enumerate() {
                let arg = match data_sign(decls, p1, j) {
                    Sign::Pos => join_ty(decls, x, y)?,
                    Sign::Neg => meet_ty(decls, x, y)?,
                    Sign::Inv | Sign::Bi if x == y => (*x).clone(),
                    _ => return None,
                };
                out = Type::app(out, arg);
            }
            out
        }
        _ => return None,
    };
    Some(if caps.is_empty() {
        inner
    } else {
        Type::Captured(Rc::new(inner), caps)
    })
}

/// Greatest lower bound of two types (used when a head variable collects
/// multiple upper bounds under covariant matching).
fn meet_ty(decls: &Decls, a: &Type, b: &Type) -> Option<Type> {
    if ty_le(decls, a, b) {
        return Some(a.clone());
    }
    if ty_le(decls, b, a) {
        return Some(b.clone());
    }
    let (ba, ca) = split_caps(a);
    let (bb, cb) = split_caps(b);
    let caps = Caps {
        atoms: ca.atoms.intersection(&cb.atoms).cloned().collect(),
        vars: ca.vars.intersection(&cb.vars).cloned().collect(),
    };
    let inner = match (ba, bb) {
        (Type::Fun(p1, s1, r1), Type::Fun(p2, s2, r2)) => {
            let stage = match (s1, s2) {
                (Stage::Pure, _) | (_, Stage::Pure) => Stage::Pure,
                (Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => Stage::Runtime(
                    Row {
                        elems: r1
                            .elems
                            .iter()
                            .filter(|(k, v)| r2.elems.get(*k) == Some(*v))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                        vars: r1.vars.intersection(&r2.vars).cloned().collect(),
                        less: BTreeSet::new(),
                    },
                    Row {
                        elems: e1
                            .elems
                            .iter()
                            .filter(|(k, v)| e2.elems.get(*k) == Some(*v))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                        vars: e1.vars.intersection(&e2.vars).cloned().collect(),
                        less: BTreeSet::new(),
                    },
                ),
            };
            Type::fun(join_ty(decls, p1, p2)?, stage, meet_ty(decls, r1, r2)?)
        }
        (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
            let met: Option<Vec<Type>> = xs
                .iter()
                .zip(ys.iter())
                .map(|(x, y)| meet_ty(decls, x, y))
                .collect();
            Type::Tuple(Rc::from(met?))
        }
        (Type::Record(f), Type::Record(g))
            if f.tail.is_none() && g.tail.is_none() && f.fields.keys().eq(g.fields.keys()) =>
        {
            let mut fields = BTreeMap::new();
            for (k, x) in &f.fields {
                fields.insert(k.clone(), meet_ty(decls, x, &g.fields[k])?);
            }
            Type::Record(Fields { fields, tail: None })
        }
        (Type::App(..), Type::App(..)) => {
            let (Some((p1, xs)), Some((p2, ys))) = (spine(ba), spine(bb)) else {
                return None;
            };
            if p1 != p2 || xs.len() != ys.len() {
                return None;
            }
            let mut out = Type::Con(p1.clone());
            for (j, (x, y)) in xs.iter().zip(ys.iter()).enumerate() {
                let arg = match data_sign(decls, p1, j) {
                    Sign::Pos => meet_ty(decls, x, y)?,
                    Sign::Neg => join_ty(decls, x, y)?,
                    Sign::Inv | Sign::Bi if x == y => (*x).clone(),
                    _ => return None,
                };
                out = Type::app(out, arg);
            }
            out
        }
        _ => return None,
    };
    Some(if caps.is_empty() {
        inner
    } else {
        Type::Captured(Rc::new(inner), caps)
    })
}

/// Direction-aware one-way matching of `inst` against `query`.
///
/// Returns `Some(σ)` only when `serves(decls, query, inst.head[σ])` holds
/// (sound by final verification against [`serves`]).
pub fn match_instance(decls: &Decls, inst: &InstanceDecl, query: &Constraint) -> Option<TypeSubst> {
    if inst.head.class != query.class || inst.head.args.len() != query.args.len() {
        return None;
    }
    let head_vars: HashSet<Name> = inst.vars.iter().map(|(v, _)| v.clone()).collect();
    let mut bounds = VarBounds::default();
    for (j, (h, q)) in inst.head.args.iter().zip(&query.args).enumerate() {
        let dir = match class_sign(decls, &query.class, j) {
            Sign::Pos => Dir::Sub,
            Sign::Neg => Dir::Sup,
            Sign::Inv | Sign::Bi => Dir::Eq,
        };
        if !collect_match(decls, &head_vars, h, q, dir, &mut bounds) {
            return None;
        }
    }
    let mut sigma = TypeSubst::new();
    for (v, k) in &inst.vars {
        match k {
            Kind::Services | Kind::Errors => {
                let row = bounds.rows.get(v).cloned().unwrap_or_else(Row::none);
                sigma.insert(v.clone(), SubstVal::Row(row));
            }
            Kind::Captures => {
                let caps = bounds.caps.get(v).cloned().unwrap_or_default();
                sigma.insert(v.clone(), SubstVal::Caps(caps));
            }
            _ => {
                let chosen = if let Some(t) = bounds.exact.get(v) {
                    t.clone()
                } else if let Some(lows) = bounds.lower.get(v) {
                    let mut acc = lows[0].clone();
                    for t in &lows[1..] {
                        acc = join_ty(decls, &acc, t)?;
                    }
                    acc
                } else if let Some(highs) = bounds.upper.get(v) {
                    let mut acc = highs[0].clone();
                    for t in &highs[1..] {
                        acc = meet_ty(decls, &acc, t)?;
                    }
                    acc
                } else {
                    continue;
                };
                sigma.insert(v.clone(), SubstVal::Type(chosen));
            }
        }
    }
    let instantiated = inst.head.subst(decls, &sigma);
    serves(decls, query, &instantiated).then_some(sigma)
}
