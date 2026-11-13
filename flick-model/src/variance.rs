//! The variance of each parameter of a declared type (`regions.md` §2.2.1,
//! Lean `Sig.var` with `Sig.var_ok`): the least fixed point over the
//! declarations, read from the fields of the constructors. Subtyping
//! compares the arguments of data by it.

use std::collections::HashMap;

use crate::decls::Decls;
use crate::syntax::{Name, Path};
use crate::types::{Caps, Kind, Row, Stage, Type, Var};

/// `+`, `−`, `inv` or `bi` (unused).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    Bi,
    Pos,
    Neg,
    Inv,
}

impl Sign {
    fn join(self, other: Sign) -> Sign {
        match (self, other) {
            (Sign::Bi, s) | (s, Sign::Bi) => s,
            (a, b) if a == b => a,
            _ => Sign::Inv,
        }
    }
}

/// What the positions of a parameter in the fields allow, joined. A `+`
/// parameter stands under no flip (Lean `Ty.Pol.arrow`): there its argument
/// would grow what the field reaches where it must shrink.
struct Uses(Sign);

impl Uses {
    fn at(&mut self, q: bool, flipped: bool) {
        self.join(match (q, flipped) {
            (true, false) => Sign::Pos,
            (true, true) => Sign::Inv,
            (false, _) => Sign::Neg,
        });
    }

    fn join(&mut self, s: Sign) {
        self.0 = self.0.join(s);
    }

    /// Where only `≡` relates its argument, if it stands there.
    fn fixed(&mut self, stands: bool) {
        if stands {
            self.join(Sign::Inv);
        }
    }
}

/// `T ū` as `T` and `ū`.
pub fn spine(t: &Type) -> Option<(&Path, Vec<&Type>)> {
    let mut args = Vec::new();
    let mut head = t;
    while let Type::App(h, a) = head {
        args.push(&**a);
        head = h;
    }
    args.reverse();
    match head {
        Type::Con(p) => Some((p, args)),
        _ => None,
    }
}

fn in_caps(a: &Name, c: &Caps) -> bool {
    let named = |v: &Var| matches!(v, Var::Named(n) if n == a);
    c.atoms.iter().any(named) || c.vars.iter().any(named)
}

fn in_row(a: &Name, r: &Row) -> bool {
    r.vars.iter().any(|v| matches!(v, Var::Named(n) if n == a))
        || r.less
            .iter()
            .any(|(v, _)| matches!(v, Var::Named(n) if n == a))
        || r.elems.values().any(|ts| ts.iter().any(|t| occurs(a, t)))
}

fn in_stage(a: &Name, s: &Stage) -> bool {
    match s {
        Stage::Pure => false,
        Stage::Runtime(r, e) => in_row(a, r) || in_row(a, e),
    }
}

/// Whether `a` stands anywhere in `t`, rows and qualifiers included.
fn occurs(a: &Name, t: &Type) -> bool {
    match t {
        Type::Var(Var::Named(n)) => n == a,
        Type::Var(Var::Meta(_)) | Type::Con(_) => false,
        Type::App(h, x) => occurs(a, h) || occurs(a, x),
        Type::Fun(p, s, r) => occurs(a, p) || in_stage(a, s) || occurs(a, r),
        Type::Eff(r, e, t) => in_row(a, r) || in_row(a, e) || occurs(a, t),
        Type::EffOf(r, e) => in_row(a, r) || in_row(a, e),
        Type::Ref(t) => occurs(a, t),
        Type::Layer(i, e, o) => in_row(a, i) || in_row(a, e) || in_row(a, o),
        Type::Record(f) => {
            f.tail
                .as_ref()
                .is_some_and(|v| matches!(v, Var::Named(n) if n == a))
                || f.fields.values().any(|t| occurs(a, t))
        }
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().any(|t| occurs(a, t)),
        Type::Errors(r) | Type::Impl(r) | Type::Tag(r) | Type::Env(r) => in_row(a, r),
        Type::Forall(s) => !s.vars.iter().any(|(v, _)| v == a) && occurs(a, &s.body),
        Type::Captured(t, c) | Type::Bounded(t, c) => in_caps(a, c) || occurs(a, t),
    }
}

/// Where `a` stands in `t`, at polarity `q` (`true`: covariant), `flipped`
/// once a flip is above it, by the current signs of other data.
fn walk(
    signs: &HashMap<Path, Vec<Sign>>,
    a: &Name,
    t: &Type,
    q: bool,
    flipped: bool,
    out: &mut Uses,
) {
    match t {
        Type::Var(Var::Named(n)) if n == a => out.at(q, flipped),
        Type::Fun(p, s, r) => {
            walk(signs, a, p, !q, true, out);
            out.fixed(in_stage(a, s));
            walk(signs, a, r, q, flipped, out);
        }
        Type::Eff(r, e, t) => {
            out.fixed(in_row(a, r) || in_row(a, e));
            walk(signs, a, t, q, flipped, out);
        }
        Type::Captured(t, c) | Type::Bounded(t, c) => {
            if in_caps(a, c) {
                out.at(q, flipped);
            }
            walk(signs, a, t, q, flipped, out);
        }
        Type::Record(f)
            if f.tail
                .as_ref()
                .is_none_or(|v| !matches!(v, Var::Named(n) if n == a)) =>
        {
            f.fields
                .values()
                .for_each(|t| walk(signs, a, t, q, flipped, out));
        }
        Type::Tuple(ts) => ts.iter().for_each(|t| walk(signs, a, t, q, flipped, out)),
        Type::App(..) => match spine(t) {
            Some((p, args)) => {
                let theirs = signs.get(p);
                for (j, x) in args.into_iter().enumerate() {
                    // an unused parameter holds `a` only where `a` itself
                    // may stand (Lean `Ty.Pol.data`)
                    match theirs.and_then(|s| s.get(j)).copied().unwrap_or(Sign::Inv) {
                        Sign::Pos | Sign::Bi => walk(signs, a, x, q, flipped, out),
                        Sign::Neg => walk(signs, a, x, !q, true, out),
                        Sign::Inv => out.fixed(occurs(a, x)),
                    }
                }
            }
            None => out.fixed(occurs(a, t)),
        },
        _ => out.fixed(occurs(a, t)),
    }
}

/// A type subtyping moves neither up nor down (Lean `Ty.Closed`): a scalar,
/// or a nominal type without parameters, representation or hidden storage
/// whose fields are such types (a cycle of them is).
fn closed(d: &Decls, t: &Type, seen: &mut Vec<Path>) -> bool {
    let Type::Con(p) = t else { return false };
    if d.opaque.contains_key(p) || d.hidden.contains(p) || d.params.contains_key(p) {
        return false;
    }
    let Some(variants) = d.variants.get(p) else {
        return true;
    };
    if seen.contains(p) {
        return true;
    }
    seen.push(p.clone());
    let all = variants.iter().all(|(name, _)| {
        (d.constructors.get(&Path::new(&p.module, name)))
            .is_some_and(|s| fields(&s.body).0.iter().all(|f| closed(d, bare(f), seen)))
    });
    seen.pop();
    all
}

/// The fields of a constructor's type, and the type it builds.
fn fields(mut t: &Type) -> (Vec<&Type>, &Type) {
    let mut out = Vec::new();
    while let Type::Fun(x, _, r) = t {
        out.push(&**x);
        t = r;
    }
    (out, t)
}

/// `t` without the qualifiers of a value of it.
fn bare(mut t: &Type) -> &Type {
    while let Type::Captured(inner, _) | Type::Bounded(inner, _) = t {
        t = inner;
    }
    t
}

/// The signs of the declared types of `d`, by parameter, and of the
/// built-in list, which is immutable and has no declaration.
pub fn signs(d: &Decls) -> HashMap<Path, Vec<Sign>> {
    let types: Vec<&Path> = (d.variants.keys())
        .filter(|p| !d.errors.contains(*p) && d.params.contains_key(*p))
        .collect();
    let mut signs: HashMap<Path, Vec<Sign>> = types
        .iter()
        .map(|p| ((*p).clone(), vec![Sign::Bi; d.params[*p].len()]))
        .collect();
    loop {
        let mut changed = false;
        for p in &types {
            let next = sign_of(d, &signs, p);
            let old = &signs[*p];
            let joined: Vec<Sign> = old.iter().zip(&next).map(|(a, b)| a.join(*b)).collect();
            if &joined != old {
                signs.insert((*p).clone(), joined);
                changed = true;
            }
        }
        if !changed {
            signs
                .entry(Path::new("Prelude", "List"))
                .or_insert_with(|| vec![Sign::Pos]);
            return signs;
        }
    }
}

/// The signs the constructors of `p` justify, given the signs of the others.
fn sign_of(d: &Decls, signs: &HashMap<Path, Vec<Sign>>, p: &Path) -> Vec<Sign> {
    let kinds = &d.params[p];
    let mut uses: Vec<Uses> = kinds.iter().map(|_| Uses(Sign::Bi)).collect();
    let mut fixed = vec![false; kinds.len()];
    for (name, _) in &d.variants[p] {
        let Some(s) = d.constructors.get(&Path::new(&p.module, name)) else {
            return vec![Sign::Inv; kinds.len()];
        };
        let (fields, t) = fields(&s.body);
        let res = spine(bare(t)).map(|(_, args)| args).unwrap_or_default();
        for (j, u) in uses.iter_mut().enumerate() {
            let Some(r) = res.get(j) else {
                u.join(Sign::Inv);
                continue;
            };
            let others = res.iter().enumerate().filter(|(m, _)| *m != j);
            match r {
                // an ordinary variable of the constructor, which nothing
                // else it builds fixes (A1)
                Type::Var(Var::Named(v)) if !others.clone().any(|(_, o)| occurs(v, o)) => {
                    for f in &fields {
                        walk(signs, v, f, true, false, u);
                    }
                }
                // a GADT constructor that fixes the argument to a type
                // subtyping does not move (A2)
                c if closed(d, c, &mut Vec::new()) => fixed[j] = true,
                _ => u.join(Sign::Inv),
            }
        }
    }
    kinds
        .iter()
        .zip(uses.iter().zip(&fixed))
        .map(|(k, (u, f))| match (k, u.0) {
            (Kind::Type, Sign::Bi) if *f => Sign::Pos,
            (Kind::Type, s) => s,
            _ => Sign::Inv,
        })
        .collect()
}
