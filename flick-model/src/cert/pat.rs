//! `match` in a certificate: the arms' patterns and the proof that they
//! cover the scrutinee's type, by the lemmas of
//! `formal/FormalCert/Match.lean`. A pattern of a field binds it or not;
//! the generator nests no further.

use crate::syntax::{Arm, Con, Lit, Name, Pat, Term};
use crate::types::Type;

use super::Unsupported;
use super::sig::Sig;
use super::term::{Cx, field as field_ty, lit, tm};

/// The constructor key of `c` in the signature.
fn key(c: &Con) -> String {
    format!("{}.{}", c.ty.module, c.name)
}

/// A field's pattern: `_` or `x`.
fn field(p: &Pat) -> Result<&'static str, Unsupported> {
    match p {
        Pat::Wild => Ok(".wild"),
        Pat::Var(_) => Ok(".var"),
        _ => Err("a nested pattern".into()),
    }
}

/// The constructor and fields of a pattern of data.
fn spine(p: &Pat) -> Option<(String, &[Pat])> {
    match p {
        Pat::Con(c, ps) => Some((key(c), ps)),
        Pat::Tuple(ps) => Some(("Tuple".into(), ps)),
        _ => None,
    }
}

/// `p` in the core, with the names it binds left to right.
fn pat(p: &Pat) -> Result<(String, Vec<Name>), Unsupported> {
    let names = |ps: &[Pat]| -> Vec<Name> {
        ps.iter()
            .filter_map(|p| match p {
                Pat::Var(x) => Some(x.clone()),
                _ => None,
            })
            .collect()
    };
    Ok(match p {
        Pat::Wild => (".wild".into(), Vec::new()),
        Pat::Var(x) => (".var".into(), vec![x.clone()]),
        Pat::Lit(l) => (format!("(.lit {})", lit(l)?), Vec::new()),
        Pat::Record(fs) => {
            let [(l, q)] = &fs[..] else {
                return Err("a record pattern of more than one field".into());
            };
            let qs = [q.clone()];
            (format!("(.rfield \"{l}\" {} .rany)", field(q)?), names(&qs))
        }
        p => {
            let (k, ps) = spine(p).ok_or("a list or or-pattern")?;
            let text = ps.iter().try_fold(format!("(.ctor {k:?})"), |f, q| {
                Ok::<_, Unsupported>(format!("(.app {f} {})", field(q)?))
            })?;
            (text, names(ps))
        }
    })
}

/// The terms of `ctx` with the names of a pattern bound before them.
fn bind(names: &[Name], ctx: &[Name]) -> Vec<Name> {
    names.iter().chain(ctx).cloned().collect()
}

/// `match s { arms }` in the core: an arm's body sees what its pattern
/// binds, leftmost innermost.
pub(super) fn match_tm(
    s: &Term,
    arms: &[Arm],
    ctx: &[Name],
    sig: &Sig,
) -> Result<String, Unsupported> {
    let mut alts = ".anil".to_owned();
    for arm in arms.iter().rev() {
        let (p, names) = pat(&arm.pat)?;
        let inner = bind(&names, ctx);
        let b = tm(&arm.body, &inner, sig)?;
        alts = match &arm.guard {
            None => format!("(.acons {p} {b} {alts})"),
            Some(g) => format!("(.aguard {p} {} {b} {alts})", tm(g, &inner, sig)?),
        };
    }
    Ok(format!("(.match_ {} {alts})", tm(s, ctx, sig)?))
}

/// The type arguments of a type of data, if it is `T ū` or a pair.
fn data(t: &Type) -> Option<(String, Vec<&Type>)> {
    match t {
        Type::Tuple(ts) => Some(("Tuple".into(), ts.iter().collect())),
        // the core writes the qualifier of no data the signature has
        // (`translate::lean::elide`)
        Type::Captured(x, _) => data(x),
        t => {
            let mut args = Vec::new();
            let mut h = t;
            while let Type::App(f, a) = h {
                args.push(&**a);
                h = f;
            }
            args.reverse();
            match h {
                Type::Con(adt) => Some((adt.to_string(), args)),
                _ => None,
            }
        }
    }
}

impl Cx<'_> {
    /// (Match): the scrutinee at its type, each arm under what its pattern
    /// binds, and the coverage of the unguarded arms.
    pub(super) fn match_(
        &mut self,
        s: &Term,
        arms: &[Arm],
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let st = self.inferred(s)?;
        let ps = self.typing(s, &st, col + 2)?;
        let boolean = Type::con("Prelude", "Bool");
        let mut alts = ".anil".to_owned();
        for arm in arms.iter().rev() {
            let (_, names) = pat(&arm.pat)?;
            let pt = self.pat_ty(&arm.pat, &st)?;
            let body = self.under_all(&names, |c| c.typing(&arm.body, t, col + 2))?;
            alts = match &arm.guard {
                None => format!("(.acons {pt}{pad}{body}{pad}{alts})"),
                Some(g) => {
                    let pg = self.under_all(&names, |c| c.typing(g, &boolean, col + 2))?;
                    format!("(.aguard {pt}{pad}{pg}{pad}{body}{pad}{alts})")
                }
            };
        }
        let cov = self.covers(arms, &st)?;
        Ok(format!("(.match_{pad}{ps}{pad}{alts}{pad}(.inl {cov}))"))
    }

    /// `check` with the names of a pattern bound, leftmost innermost.
    fn under_all<T>(&mut self, names: &[Name], check: impl FnOnce(&mut Self) -> T) -> T {
        let outer = self.terms.clone();
        self.terms = bind(names, &outer);
        let out = check(self);
        self.terms = outer;
        out
    }

    /// The type arguments of the data a pattern of `k` matches, printed.
    fn args(&self, k: &str, st: &Type) -> Result<(String, Vec<String>), Unsupported> {
        let c = self
            .sig
            .ctors
            .get(k)
            .ok_or_else(|| format!("the constructor `{k}`, not in the signature"))?;
        let (adt, args) = data(st).ok_or_else(|| format!("a pattern of `{k}` at `{st}`"))?;
        if adt != c.adt || args.len() != c.k {
            return Err(format!("a pattern of `{k}` at `{st}`"));
        }
        let us = args.iter().map(|a| self.ty(a)).collect::<Result<_, _>>()?;
        Ok((adt, us))
    }

    /// `⊢ p : st ⇒ Δ`.
    fn pat_ty(&self, p: &Pat, st: &Type) -> Result<String, Unsupported> {
        Ok(match p {
            Pat::Wild => ".wild".into(),
            Pat::Var(_) => ".var".into(),
            Pat::Lit(l) => lit(l).map(|_| ".lit".to_owned())?,
            Pat::Record(fs) => {
                let [(l, q)] = &fs[..] else {
                    return Err("a record pattern of more than one field".into());
                };
                let ft = field_ty(st, l)?;
                format!(
                    "(.rfield (R := {}) (t := {}) rfl {} .rany)",
                    self.ty(st)?,
                    self.ty(ft)?,
                    field(q)?
                )
            }
            p => {
                let (k, ps) = spine(p).ok_or("a list or or-pattern")?;
                let (adt, us) = self.args(&k, st)?;
                let c = &self.sig.ctors[&k];
                let head = format!(
                    "(PatTy.head_plain S (K := {k:?}) (ts := [{}]) (T := {adt:?}) (us := [{}]) ((hS.ctor _ rfl).trans rfl) ((hS.res _ rfl).trans rfl) rfl)",
                    c.fields.join(", "),
                    us.join(", ")
                );
                let app = ps.iter().try_fold(head, |f, q| {
                    Ok::<_, Unsupported>(format!("(.app {f} {})", field(q)?))
                })?;
                format!("(.adt {app} (Ty.tcon_hd _ _))")
            }
        })
    }

    /// `Exh(st, unguarded arms)` by the shape of the arms.
    fn covers(&self, arms: &[Arm], st: &Type) -> Result<String, Unsupported> {
        let ps: Vec<&Pat> = arms
            .iter()
            .filter(|a| a.guard.is_none())
            .map(|a| &a.pat)
            .collect();
        let any = |f: fn(&Pat) -> bool| ps.iter().any(|p| f(p));
        Ok(if any(|p| matches!(p, Pat::Wild)) {
            "(exh_wild (by decide))".into()
        } else if any(|p| matches!(p, Pat::Var(_))) {
            "(exh_var (by decide))".into()
        } else if any(|p| matches!(p, Pat::Lit(Lit::Unit))) {
            "(Exh.mono exh_unit (by decide))".into()
        } else if any(|p| matches!(p, Pat::Lit(Lit::Bool(_)))) {
            "(Exh.mono exh_bool (by decide))".into()
        } else if let Some(Pat::Record(fs)) = ps.first() {
            let [(l, q)] = &fs[..] else {
                return Err("a record pattern of more than one field".into());
            };
            format!(
                "(exh_record (R := {}) (l := \"{l}\") (q := {}) (by decide) (by decide) (by decide))",
                self.ty(st)?,
                field(q)?
            )
        } else if let Some((k, _)) = ps.first().and_then(|p| spine(p)) {
            let (adt, us) = self.args(&k, st)?;
            format!(
                "(exh_gen hS (T := {adt:?}) (us := [{}]) (by decide) (by decide))",
                us.join(", ")
            )
        } else {
            return Err("a match of no shape the certificates cover".into());
        })
    }
}
