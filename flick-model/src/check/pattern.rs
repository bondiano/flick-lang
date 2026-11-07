//! Patterns (`docs/formal/core.md` §2.4): `⊢ p : τ ⇒ Δ`, and `Exh(τ, Π)`
//! by Maranget's usefulness of a wildcard: `Π` covers `τ` when no value
//! is left that a wildcard after `Π` would be the first to match.

use std::collections::BTreeMap;
use std::mem;
use std::rc::Rc;
use std::slice;

use super::generalize::Inst;
use super::{Check, Checker, Solved, error};
use crate::syntax::{Con, Lit, Name, Pat, Path};
use crate::types::{Fields, Kind, Scheme, Type, Var};

/// A constructor of the values of a type, as `Exh` splits them.
#[derive(Clone, Debug, PartialEq)]
enum Ctor {
    Con(Rc<Con>),
    Lit(Lit),
    Tuple(usize),
    Record(Vec<Name>),
    Nil,
    Cons,
}

/// A pattern as `Exh` reads it: lists as `Cons`/`Nil`, records by label.
#[derive(Clone, Debug)]
enum P {
    Wild,
    Con(Ctor, Vec<P>),
    Record(BTreeMap<Name, P>),
    Or(Vec<P>),
}

fn normal(p: &Pat) -> P {
    match p {
        Pat::Wild | Pat::Var(_) => P::Wild,
        Pat::Lit(l) => P::Con(Ctor::Lit(l.clone()), Vec::new()),
        Pat::Con(c, ps) => P::Con(Ctor::Con(c.clone()), ps.iter().map(normal).collect()),
        Pat::Tuple(ps) => P::Con(Ctor::Tuple(ps.len()), ps.iter().map(normal).collect()),
        Pat::List(ps, rest) => ps.iter().rev().fold(
            rest.as_ref()
                .map_or(P::Con(Ctor::Nil, Vec::new()), |r| normal(r)),
            |tail, p| P::Con(Ctor::Cons, vec![normal(p), tail]),
        ),
        Pat::Record(fs) => P::Record(fs.iter().map(|(l, p)| (l.clone(), normal(p))).collect()),
        Pat::Or(ps) => P::Or(ps.iter().map(normal).collect()),
    }
}

fn list(t: Type) -> Type {
    Type::app(Type::con("Prelude", "List"), t)
}

fn lit_type(l: &Lit) -> Type {
    Type::con(
        "Prelude",
        match l {
            Lit::Int(_) => "Int",
            Lit::Float(_) => "Float",
            Lit::String(_) => "String",
            Lit::Bool(_) => "Bool",
            Lit::Unit => "Unit",
        },
    )
}

impl Checker<'_> {
    /// `⊢ p : τ ⇒ Δ`, the names of `Δ` distinct.
    pub(super) fn pattern(&mut self, p: &Pat, t: &Type) -> Check<Vec<(Name, Type)>> {
        let mut out = Vec::new();
        self.bind_pattern(p, t, &mut out)?;
        for (i, (x, _)) in out.iter().enumerate() {
            if out[..i].iter().any(|(y, _)| y == x) {
                return error("Pattern", format!("`{x}` is bound twice"));
            }
        }
        Ok(out)
    }

    fn bind_pattern(&mut self, p: &Pat, t: &Type, out: &mut Vec<(Name, Type)>) -> Check<()> {
        if !matches!(p, Pat::Var(_) | Pat::Wild) && !matches!(self.qualifier(t), q if q.is_empty())
        {
            // (Proj): the parts of `(T ῡ)^C` have `C` too
            let whole = t.clone();
            let bare = self.resolve(t);
            let mut parts = Vec::new();
            self.bind_pattern(p, &bare, &mut parts)?;
            out.extend(
                parts
                    .into_iter()
                    .map(|(x, part)| (x, self.project_part(&whole, part))),
            );
            return Ok(());
        }
        let full = t.clone();
        let t = &self.resolve(t);
        match p {
            Pat::Wild => Ok(()),
            Pat::Var(x) => {
                out.push((x.clone(), full));
                Ok(())
            }
            Pat::Lit(l) => self.unify(t, &lit_type(l)),
            Pat::Con(c, ps) => {
                let params = self.con_fields(c, t)?;
                if params.len() != ps.len() {
                    return error(
                        "Pattern",
                        format!("`{}` takes {} fields", c.name, params.len()),
                    );
                }
                ps.iter()
                    .zip(params)
                    .try_for_each(|(p, t)| self.bind_pattern(p, &t, out))
            }
            Pat::Tuple(ps) => {
                let ts: Vec<Type> = ps.iter().map(|_| self.fresh()).collect();
                self.unify(t, &Type::Tuple(ts.clone().into()))?;
                ps.iter()
                    .zip(ts)
                    .try_for_each(|(p, t)| self.bind_pattern(p, &t, out))
            }
            Pat::List(ps, rest) => {
                let x = self.fresh();
                self.unify(t, &list(x.clone()))?;
                ps.iter().try_for_each(|p| self.bind_pattern(p, &x, out))?;
                match rest {
                    Some(r) => self.bind_pattern(r, &list(x), out),
                    None => Ok(()),
                }
            }
            Pat::Record(fs) => {
                let types: BTreeMap<Name, Type> =
                    fs.iter().map(|(l, _)| (l.clone(), self.fresh())).collect();
                let open = Fields {
                    fields: types.clone(),
                    tail: Some(Var::Meta(self.meta())),
                };
                self.unify(t, &Type::Record(open))?;
                fs.iter()
                    .try_for_each(|(l, p)| self.bind_pattern(p, &types[l], out))
            }
            // the alternatives bind the same names at one type (`docs/formal/core.md#or-pattern-alternatives-are-compared-by-subtyping`)
            Pat::Or(ps) => {
                let mut first: Option<Vec<(Name, Type)>> = None;
                for p in ps.iter() {
                    let mut bound = Vec::new();
                    self.bind_pattern(p, t, &mut bound)?;
                    bound.sort_by(|a, b| a.0.cmp(&b.0));
                    match &first {
                        None => first = Some(bound),
                        Some(prev) => {
                            if prev.len() != bound.len()
                                || prev.iter().zip(&bound).any(|(a, b)| a.0 != b.0)
                            {
                                return error("Pattern", "the alternatives bind different names");
                            }
                            for (a, b) in prev.clone().iter().zip(&bound) {
                                self.unify(&a.1, &b.1)?;
                            }
                        }
                    }
                }
                out.extend(first.unwrap_or_default());
                Ok(())
            }
        }
    }

    /// The types of the fields of `K` at `T ῡ`. A constructor of a GADT is
    /// matched under the substitution θ its result and `T ῡ` unify by: the
    /// rigid variables it fixes are equalities of the arm, and those it
    /// leaves open, the types it hides, are rigid in it.
    fn con_fields(&mut self, c: &Con, t: &Type) -> Check<Vec<Type>> {
        let Some(scheme) = self
            .decls
            .constructors
            .get(&Path::new(&c.ty.module, &c.name))
            .cloned()
        else {
            return error(
                "Var",
                format!("no constructor `{}.{}`", c.ty.module, c.name),
            );
        };
        let gadt = indexed(&scheme);
        let (mut ct, insts) = self.instantiate_with(&scheme);
        let mut params = Vec::new();
        while let Type::Fun(p, _, r) = ct {
            params.push((*p).clone());
            ct = (*r).clone();
        }
        let refining = mem::replace(&mut self.refining, gadt);
        let unified = self.unify(&self.resolve(&ct), &self.resolve(t));
        self.refining = refining;
        unified?;
        if gadt {
            for (inst, (n, _)) in insts.iter().zip(&scheme.vars) {
                if let Inst::Type(Type::Var(Var::Meta(m))) = inst
                    && matches!(self.follow(&Type::Var(Var::Meta(*m))), Type::Var(Var::Meta(x)) if x == *m)
                {
                    self.rigid += 1;
                    let hidden =
                        Type::Var(Var::Named(format!("{n}'{}", self.rigid).as_str().into()));
                    self.bind(*m, Solved::Type(hidden.clone()), &hidden)?;
                }
            }
        }
        Ok(params)
    }

    /// `Exh(τ, Π)`.
    pub(super) fn exhaustive(&mut self, t: &Type, pats: &[&Pat]) -> Check<bool> {
        let rows = pats.iter().map(|p| vec![normal(p)]).collect();
        Ok(!self.missing(rows, slice::from_ref(t))?)
    }

    /// Whether a value of `tys` matches no row.
    fn missing(&mut self, rows: Vec<Vec<P>>, tys: &[Type]) -> Check<bool> {
        let Some((first, rest)) = tys.split_first() else {
            return Ok(rows.is_empty());
        };
        let rows = expand(rows);
        let used = |c: &Ctor| rows.iter().any(|r| head_is(&r[0], c));
        // a constructor no value of `first` is built by needs no row
        let signature = self.signature(first)?.map(|ctors| {
            ctors
                .into_iter()
                .filter(|c| {
                    let saved = self.clone();
                    let builds = self.ctor_fields(c, first).is_ok();
                    *self = saved;
                    builds
                })
                .collect::<Vec<_>>()
        });
        match signature {
            Some(ctors) if ctors.iter().all(used) => {
                for ctor in ctors {
                    // what the constructor learns holds for its own rows only
                    let saved = self.clone();
                    let fields = self.ctor_fields(&ctor, first)?;
                    let specialized: Vec<Vec<P>> = rows
                        .iter()
                        .filter_map(|r| specialize(r, &ctor, fields.len()))
                        .collect();
                    let mut tys = fields;
                    tys.extend(rest.iter().cloned());
                    let missing = self.missing(specialized, &tys);
                    *self = saved;
                    if missing? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            // a constructor no row names is covered only by the rows with a
            // wildcard: the default matrix decides, under what it learns
            Some(ctors) => {
                let default: Vec<Vec<P>> = rows
                    .iter()
                    .filter(|r| matches!(r[0], P::Wild))
                    .map(|r| r[1..].to_vec())
                    .collect();
                for ctor in ctors.iter().filter(|c| !used(c)) {
                    let saved = self.clone();
                    self.ctor_fields(ctor, first)?;
                    let missing = self.missing(default.clone(), rest);
                    *self = saved;
                    if missing? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            None => {
                let default = rows
                    .iter()
                    .filter(|r| matches!(r[0], P::Wild))
                    .map(|r| r[1..].to_vec())
                    .collect();
                self.missing(default, rest)
            }
        }
    }

    /// The constructors of the values of `t`, if there are finitely many.
    fn signature(&mut self, t: &Type) -> Check<Option<Vec<Ctor>>> {
        let t = self.zonk(&self.resolve(t));
        let mut head = &t;
        while let Type::App(h, _) = head {
            head = h;
        }
        Ok(Some(match head {
            Type::Con(p) if p == &Path::new("Prelude", "Bool") => {
                vec![Ctor::Lit(Lit::Bool(true)), Ctor::Lit(Lit::Bool(false))]
            }
            Type::Con(p) if p == &Path::new("Prelude", "Unit") => vec![Ctor::Lit(Lit::Unit)],
            Type::Con(p) if p == &Path::new("Prelude", "List") => vec![Ctor::Nil, Ctor::Cons],
            Type::Con(p) => match self.decls.variants.get(p) {
                Some(vs) => vs
                    .iter()
                    .map(|(n, arity)| {
                        Ctor::Con(Rc::new(Con {
                            ty: p.clone(),
                            name: n.clone(),
                            arity: *arity,
                            dicts: 0,
                        }))
                    })
                    .collect(),
                None => return Ok(None),
            },
            Type::Tuple(ts) => vec![Ctor::Tuple(ts.len())],
            Type::Record(f) => vec![Ctor::Record(f.fields.keys().cloned().collect())],
            Type::Errors(r) if r.vars.is_empty() && r.elems.len() == 1 => {
                let (e, _) = r.elems.iter().next().expect("one element");
                vec![Ctor::Con(Rc::new(Con {
                    ty: e.clone(),
                    name: e.name.clone(),
                    arity: self
                        .decls
                        .variants
                        .get(e)
                        .and_then(|v| v.first())
                        .map_or(0, |v| v.1),
                    dicts: 0,
                }))]
            }
            _ => return Ok(None),
        }))
    }

    fn ctor_fields(&mut self, ctor: &Ctor, t: &Type) -> Check<Vec<Type>> {
        Ok(match ctor {
            Ctor::Con(c) => self.con_fields(c, t)?,
            Ctor::Lit(_) | Ctor::Nil => Vec::new(),
            Ctor::Tuple(_) => match self.zonk(&self.resolve(t)) {
                Type::Tuple(ts) => ts.to_vec(),
                _ => Vec::new(),
            },
            Ctor::Record(labels) => match self.zonk(&self.resolve(t)) {
                Type::Record(f) => labels.iter().map(|l| f.fields[l].clone()).collect(),
                _ => Vec::new(),
            },
            Ctor::Cons => {
                let x = self.fresh();
                self.unify(t, &list(x.clone()))?;
                vec![x.clone(), list(x)]
            }
        })
    }
}

/// Whether `p` is a pattern of constructor `c`.
fn head_is(p: &P, c: &Ctor) -> bool {
    match (p, c) {
        (P::Con(Ctor::Con(a), _), Ctor::Con(b)) => a.ty == b.ty && a.name == b.name,
        (P::Con(a, _), b) => a == b,
        (P::Record(_), Ctor::Record(_)) => true,
        _ => false,
    }
}

/// The rows with the alternatives of an or-pattern in front split.
fn expand(rows: Vec<Vec<P>>) -> Vec<Vec<P>> {
    let mut out = Vec::new();
    for row in rows {
        match &row[0] {
            P::Or(ps) => {
                let split = ps.iter().map(|p| {
                    let mut r = vec![p.clone()];
                    r.extend(row[1..].iter().cloned());
                    r
                });
                out.extend(expand(split.collect()));
            }
            _ => out.push(row),
        }
    }
    out
}

/// The row for values of `ctor`, whose `n` fields come first, if it can
/// match one.
fn specialize(row: &[P], ctor: &Ctor, n: usize) -> Option<Vec<P>> {
    let head: Vec<P> = match (&row[0], ctor) {
        (P::Wild, _) => vec![P::Wild; n],
        (P::Con(Ctor::Con(c), ps), Ctor::Con(k)) if c.ty == k.ty && c.name == k.name => ps.clone(),
        (P::Con(c, ps), k) if !matches!(c, Ctor::Con(_)) && c == k => ps.clone(),
        (P::Record(fs), Ctor::Record(labels)) => labels
            .iter()
            .map(|l| fs.get(l).cloned().unwrap_or(P::Wild))
            .collect(),
        _ => return None,
    };
    let mut out = head;
    out.extend(row[1..].iter().cloned());
    Some(out)
}

/// Whether a constructor of this scheme is one of a GADT: it builds other
/// than `T ᾱ` over its first variables, or holds a dictionary.
fn indexed(s: &Scheme) -> bool {
    let mut t = &s.body;
    while let Type::Fun(p, _, r) = t {
        if matches!(**p, Type::Dict(..)) {
            return true;
        }
        t = r;
    }
    if let Type::Captured(inner, _) | Type::Bounded(inner, _) = t {
        t = inner;
    }
    let mut args = Vec::new();
    while let Type::App(h, a) = t {
        args.push(a);
        t = h;
    }
    args.reverse();
    let plain = args.len() <= s.vars.len()
        && args
            .iter()
            .zip(&s.vars)
            .all(|(a, (n, _))| matches!(&***a, Type::Var(Var::Named(x)) if x == n));
    !plain
        || s.vars[args.len().min(s.vars.len())..]
            .iter()
            .any(|(_, k)| *k != Kind::Captures)
}
