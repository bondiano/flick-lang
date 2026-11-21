//! Subtyping (`ty_le`), reach computation (`reach_of`), and capture-aware
//! substitution for the B1/K1' elaboration model.

use std::collections::BTreeSet;
use std::rc::Rc;

use super::{SubstVal, TypeSubst, data_sign, serves_args};
use crate::decls::Decls;
use crate::syntax::Path;
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};
use crate::variance::{Sign, spine};

pub(super) fn strip_outer_caps(mut t: &Type) -> &Type {
    while let Type::Captured(inner, _) | Type::Bounded(inner, _) = t {
        t = inner;
    }
    t
}

/// Bare type and union of all outer capture/bound qualifiers.
pub(super) fn split_caps(mut t: &Type) -> (&Type, Caps) {
    let mut caps = Caps::default();
    while let Type::Captured(inner, c) | Type::Bounded(inner, c) = t {
        caps = caps.union(c);
        t = inner;
    }
    (t, caps)
}

/// Bare type and union of outer hidden (`Captured`) qualifiers only (for `Stage.use`).
pub(super) fn split_hidden_caps(mut t: &Type) -> (&Type, Caps) {
    let mut caps = Caps::default();
    while let Type::Captured(inner, _) | Type::Bounded(inner, _) = t {
        if let Type::Captured(_, c) = t {
            caps = caps.union(c);
        }
        t = inner;
    }
    (t, caps)
}

pub(super) fn live_service() -> Path {
    Path::new("Prelude", "Live")
}

pub(super) fn scope_service() -> Path {
    Path::new("Prelude", "Scope")
}

/// Captures represented by `{Live C}` and `{Scope ϱ}` inside a service row.
pub(super) fn row_live_caps(r: &Row) -> Caps {
    let mut out = Caps::default();
    if let Some(args) = r.elems.get(&live_service())
        && let Some(arg) = args.first()
    {
        let (_, c) = split_caps(arg);
        out = out.union(&c);
    }
    if let Some(args) = r.elems.get(&scope_service())
        && let Some(Type::Var(v)) = args.first()
    {
        out.atoms.insert(v.clone());
    }
    out
}

/// `C ⊆ C'`.
pub fn caps_sub(a: &Caps, b: &Caps) -> bool {
    a.atoms.is_subset(&b.atoms) && a.vars.is_subset(&b.vars)
}

/// Reach set `⌈τ⌉` (`docs/formal/regions.md` §2.1, Lean `Ty.reach`).
pub fn reach_of(decls: &Decls, t: &Type) -> Caps {
    let mut out = Caps::default();
    reach_into(decls, t, &mut out);
    out
}

pub(super) fn reach_into(decls: &Decls, t: &Type, out: &mut Caps) {
    match t {
        Type::Var(v) => {
            out.atoms.insert(v.clone());
        }
        Type::App(..) => {
            let mut args = Vec::new();
            let mut head = t;
            while let Type::App(h, a) = head {
                args.push(&**a);
                head = h;
            }
            let params = match head {
                Type::Con(p) => decls.params.get(p),
                _ => None,
            };
            reach_into(decls, head, out);
            for (i, a) in args.iter().rev().enumerate() {
                if params.and_then(|ks| ks.get(i)) != Some(&Kind::Services) {
                    reach_into(decls, a, out);
                }
            }
        }
        Type::Fun(a, s, b) => {
            reach_into(decls, a, out);
            reach_into(decls, b, out);
            if let Stage::Runtime(r, _) = s {
                *out = out.union(&row_live_caps(r));
            }
        }
        Type::Eff(r, _, a) => {
            reach_into(decls, a, out);
            *out = out.union(&row_live_caps(r));
        }
        Type::Ref(a) => reach_into(decls, a, out),
        Type::Record(f) => f.fields.values().for_each(|x| reach_into(decls, x, out)),
        Type::Tuple(ts) => ts.iter().for_each(|x| reach_into(decls, x, out)),
        Type::Captured(inner, c) | Type::Bounded(inner, c) => {
            reach_into(decls, inner, out);
            *out = out.union(c);
        }
        Type::Caps(c) => *out = out.union(c),
        Type::Errors(r) => {
            for args in r.elems.values() {
                args.iter().for_each(|a| reach_into(decls, a, out));
            }
            out.atoms.extend(r.vars.iter().cloned());
        }
        Type::Forall(s) => {
            let mut inner = Caps::default();
            reach_into(decls, &s.body, &mut inner);
            let bound = |v: &Var| matches!(v, Var::Named(n) if s.vars.iter().any(|(b, _)| b == n));
            inner.atoms.retain(|v| !bound(v));
            inner.vars.retain(|v| !bound(v));
            *out = out.union(&inner);
        }
        // Nominal constructors, services, layers, implementations, tags,
        // environments and dictionaries carry no captures (`regions.md` §2.1).
        Type::Con(_)
        | Type::EffOf(..)
        | Type::Layer(..)
        | Type::Impl(_)
        | Type::Tag(_)
        | Type::Dict(..)
        | Type::Env(_) => {}
    }
}

pub(super) fn row_without_live(r: &Row) -> Row {
    let mut out = r.clone();
    out.elems.remove(&live_service());
    out
}

pub(super) fn row_use_within(r1: &Row, c1: &Caps, r2: &Row, c2: &Caps) -> bool {
    let live1 = row_live_caps(r1).union(c1);
    let live2 = row_live_caps(r2).union(c2);
    row_without_live(r1).within(&row_without_live(r2)) && caps_sub(&live1, &live2)
}

pub(super) fn stage_use_within(s1: &Stage, c1: &Caps, s2: &Stage, c2: &Caps) -> bool {
    match (s1, s2) {
        (Stage::Pure, _) => true,
        (Stage::Runtime(..), Stage::Pure) => false,
        (Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => {
            row_use_within(r1, c1, r2, c2) && e1.within(e2)
        }
    }
}

/// Core subtyping `τ ≤ τ'` matching Lean `LeLive` and elaboration `Ty.Le`.
pub fn ty_le(decls: &Decls, sub: &Type, sup: &Type) -> bool {
    if sub == sup {
        return true;
    }
    // `LeLive.reach`: every subtype step preserves reach.
    if !caps_sub(&reach_of(decls, sub), &reach_of(decls, sup)) {
        return false;
    }
    let (a, ca) = split_caps(sub);
    let (b, hb) = split_hidden_caps(sup);
    match (a, b) {
        (Type::Fun(p1, s1, r1), Type::Fun(p2, s2, r2)) => {
            ty_le(decls, p2, p1) && stage_use_within(s1, &ca, s2, &hb) && ty_le(decls, r1, r2)
        }
        (Type::Eff(r1, e1, t1), Type::Eff(r2, e2, t2)) => {
            row_use_within(r1, &ca, r2, &hb) && e1.within(e2) && ty_le(decls, t1, t2)
        }
        (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
            xs.iter().zip(ys.iter()).all(|(x, y)| ty_le(decls, x, y))
        }
        (Type::Record(f), Type::Record(g))
            if f.tail.is_none() && g.tail.is_none() && f.fields.keys().eq(g.fields.keys()) =>
        {
            f.fields
                .values()
                .zip(g.fields.values())
                .all(|(x, y)| ty_le(decls, x, y))
        }
        (Type::App(..), Type::App(..)) => {
            let (Some((p1, xs)), Some((p2, ys))) = (spine(a), spine(b)) else {
                return a == b;
            };
            if p1 != p2 || xs.len() != ys.len() {
                return false;
            }
            xs.iter().zip(ys.iter()).enumerate().all(|(j, (x, y))| {
                if !caps_sub(&reach_of(decls, x), &reach_of(decls, y)) {
                    return false;
                }
                match data_sign(decls, p1, j) {
                    Sign::Pos => ty_le(decls, x, y),
                    Sign::Neg => ty_le(decls, y, x),
                    Sign::Inv | Sign::Bi => x == y,
                }
            })
        }
        (Type::Dict(c1, xs), Type::Dict(c2, ys)) if c1 == c2 && xs.len() == ys.len() => {
            serves_args(decls, c1, ys, xs)
        }
        _ => a == b,
    }
}

/// Applies `sigma` to `t`, expanding capture substitutions via `reach_of`.
pub fn subst_type(decls: &Decls, t: &Type, sigma: &TypeSubst) -> Type {
    if sigma.is_empty() {
        return t.clone();
    }
    match t {
        Type::Var(Var::Named(n)) => match sigma.get(n) {
            Some(SubstVal::Type(ty)) => ty.clone(),
            Some(SubstVal::Caps(c)) => Type::Caps(c.clone()),
            _ => t.clone(),
        },
        Type::Var(Var::Meta(_)) | Type::Con(_) => t.clone(),
        Type::App(h, a) => Type::app(subst_type(decls, h, sigma), subst_type(decls, a, sigma)),
        Type::Fun(p, s, r) => Type::fun(
            subst_type(decls, p, sigma),
            subst_stage(decls, s, sigma),
            subst_type(decls, r, sigma),
        ),
        Type::Eff(r, e, a) => Type::Eff(
            subst_row(decls, r, sigma),
            subst_row(decls, e, sigma),
            Rc::new(subst_type(decls, a, sigma)),
        ),
        Type::EffOf(r, e) => Type::EffOf(subst_row(decls, r, sigma), subst_row(decls, e, sigma)),
        Type::Ref(a) => Type::Ref(Rc::new(subst_type(decls, a, sigma))),
        Type::Layer(i, e, o) => Type::Layer(
            subst_row(decls, i, sigma),
            subst_row(decls, e, sigma),
            subst_row(decls, o, sigma),
        ),
        Type::Record(f) => Type::Record(Fields {
            fields: f
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), subst_type(decls, v, sigma)))
                .collect(),
            tail: f.tail.clone(),
        }),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|x| subst_type(decls, x, sigma)).collect()),
        Type::Errors(r) => Type::Errors(subst_row(decls, r, sigma)),
        Type::Caps(c) => Type::Caps(subst_caps(decls, c, sigma)),
        Type::Impl(r) => Type::Impl(subst_row(decls, r, sigma)),
        Type::Tag(r) => Type::Tag(subst_row(decls, r, sigma)),
        Type::Dict(c, args) => Type::Dict(
            c.clone(),
            args.iter().map(|a| subst_type(decls, a, sigma)).collect(),
        ),
        Type::Env(r) => Type::Env(subst_row(decls, r, sigma)),
        Type::Forall(s) => {
            let mut inner = sigma.clone();
            for (v, _) in &s.vars {
                inner.remove(v);
            }
            Type::Forall(Rc::new(Scheme {
                vars: s.vars.clone(),
                body: subst_type(decls, &s.body, &inner),
                outlives: s
                    .outlives
                    .iter()
                    .map(|(c, ty)| (subst_caps(decls, c, &inner), subst_type(decls, ty, &inner)))
                    .collect(),
            }))
        }
        Type::Captured(inner, c) => Type::Captured(
            Rc::new(subst_type(decls, inner, sigma)),
            subst_caps(decls, c, sigma),
        ),
        Type::Bounded(inner, c) => Type::Bounded(
            Rc::new(subst_type(decls, inner, sigma)),
            subst_caps(decls, c, sigma),
        ),
    }
}

pub(super) fn subst_caps(decls: &Decls, c: &Caps, sigma: &TypeSubst) -> Caps {
    let mut out = Caps::default();
    for atom in &c.atoms {
        match atom {
            Var::Named(n) => match sigma.get(n) {
                Some(SubstVal::Type(t)) => out = out.union(&reach_of(decls, t)),
                Some(SubstVal::Caps(c2)) => out = out.union(c2),
                _ => {
                    out.atoms.insert(atom.clone());
                }
            },
            Var::Meta(_) => {
                out.atoms.insert(atom.clone());
            }
        }
    }
    for var in &c.vars {
        match var {
            Var::Named(n) => match sigma.get(n) {
                Some(SubstVal::Caps(c2)) => out = out.union(c2),
                Some(SubstVal::Type(t)) => out = out.union(&reach_of(decls, t)),
                _ => {
                    out.vars.insert(var.clone());
                }
            },
            Var::Meta(_) => {
                out.vars.insert(var.clone());
            }
        }
    }
    out
}

pub(super) fn subst_row(decls: &Decls, r: &Row, sigma: &TypeSubst) -> Row {
    let mut out = Row::none();
    for (name, args) in &r.elems {
        let mapped: Rc<[Type]> = args.iter().map(|a| subst_type(decls, a, sigma)).collect();
        out.elems.insert(name.clone(), mapped);
    }
    for v in &r.vars {
        let gone: BTreeSet<Path> = r.less_of(v).cloned().collect();
        match v {
            Var::Named(n) => match sigma.get(n) {
                Some(SubstVal::Row(r2)) => out.add_less(r2, &gone),
                _ => out.add_var(v, &gone),
            },
            Var::Meta(_) => out.add_var(v, &gone),
        }
    }
    out
}

pub(super) fn subst_stage(decls: &Decls, s: &Stage, sigma: &TypeSubst) -> Stage {
    match s {
        Stage::Pure => Stage::Pure,
        Stage::Runtime(r, e) => {
            Stage::Runtime(subst_row(decls, r, sigma), subst_row(decls, e, sigma))
        }
    }
}

// ---------------------------------------------------------------------------
// Direction-aware one-way instance matching (`Ent.inst` under B1)
// ---------------------------------------------------------------------------
