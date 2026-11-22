//! Fields in a certificate: (Proj) and (Update) of a record, (Rec) of a
//! record literal, and (Dproj) of a dictionary, a method or a superclass.

use std::collections::BTreeMap;

use crate::syntax::{Name, Term};
use crate::types::{Fields, Type, Var};

use super::Unsupported;
use super::sig::{Sig, bare};
use super::term::{Cx, push, tm};

/// The record literal `fs` in the core: canonical UTF-16 `.rcons` order, with
/// source-order `.let_` bindings when out-of-order fields contain non-values.
pub(super) fn record_tm(
    fs: &[(Name, Term)],
    ctx: &[Name],
    sig: &Sig,
) -> Result<String, Unsupported> {
    if needs_source_lets(fs) {
        let mut indexed: Vec<(usize, &Name)> =
            fs.iter().enumerate().map(|(i, (l, _))| (i, l)).collect();
        indexed.sort_by(|a, b| a.1.encode_utf16().cmp(b.1.encode_utf16()));
        let n = fs.len();
        let mut out = ".rnil".to_owned();
        for (i, l) in indexed.into_iter().rev() {
            let db = n - 1 - i;
            out = format!("(.rcons \"{l}\" (.var {db}) {out})");
        }
        let mut nested_ctx = ctx.to_vec();
        let mut rhs_tms = Vec::with_capacity(n);
        for (i, (_, x)) in fs.iter().enumerate() {
            rhs_tms.push(tm(x, &nested_ctx, sig)?);
            nested_ctx = push(&nested_ctx, &Name::from(format!("$rec_{i}")));
        }
        for rhs in rhs_tms.into_iter().rev() {
            out = format!("(.let_ {rhs} {out})");
        }
        Ok(out)
    } else {
        let mut out = ".rnil".to_owned();
        for (l, x) in sorted(fs).into_iter().rev() {
            out = format!("(.rcons \"{l}\" {} {out})", tm(x, ctx, sig)?);
        }
        Ok(out)
    }
}

/// Whether a record literal needs source-order `.let_` bindings to preserve
/// left-to-right evaluation when elaborating into canonical `.rcons` order.
pub(super) fn needs_source_lets(fs: &[(Name, Term)]) -> bool {
    let is_sorted = fs
        .windows(2)
        .all(|w| w[0].0.encode_utf16().le(w[1].0.encode_utf16()));
    !is_sorted && fs.iter().any(|(_, x)| !x.is_value())
}

/// The fields of a record term by label in canonical UTF-16 order.
pub(super) fn sorted(fs: &[(Name, Term)]) -> Vec<(&Name, &Term)> {
    let mut fs: Vec<_> = fs.iter().map(|(l, x)| (l, x)).collect();
    fs.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    fs
}

/// The type of field `l` of a record type.
pub(super) fn field<'a>(t: &'a Type, l: &str) -> Result<&'a Type, Unsupported> {
    let f = fields(t).ok_or("a field of no record")?;
    f.fields
        .get(l)
        .ok_or_else(|| format!("a field `{l}` the type does not have"))
}

/// The fields of a record type.
pub(super) fn fields(t: &Type) -> Option<&Fields> {
    match t {
        // a rest the check left open is empty (`Env::ground`)
        Type::Record(f) if matches!(f.tail, None | Some(Var::Meta(_))) => Some(f),
        Type::Captured(x, _) | Type::Bounded(x, _) => fields(x),
        _ => None,
    }
}

impl Cx<'_> {
    /// Whether `d` is a dictionary: what a method is a field of.
    pub(super) fn is_dict(&self, d: &Term) -> bool {
        self.inferred(d)
            .is_ok_and(|t| matches!(bare(&t), Type::Dict(..)))
    }

    /// `r.l`: (Proj) of a record, or (Dproj) of a dictionary.
    pub(super) fn field(
        &mut self,
        e: &Term,
        r: &Term,
        l: &str,
        col: usize,
    ) -> Result<String, Unsupported> {
        if self.is_dict(r) {
            return self.dproj(e, r, l, col);
        }
        let pad = format!("\n{}", " ".repeat(col + 2));
        let at = self.inferred(r)?;
        let ft = field(&at, l)?;
        let (rt, ftp) = (self.ty(&at)?, self.ty(ft)?);
        let pr = self.typing(r, &at, col + 2)?;
        Ok(format!(
            "(.proj (R := {rt}) (t := {ftp}){pad}{pr}{pad}rfl (by le_live))"
        ))
    }

    /// (Update) of field `l` at the record type `t`.
    pub(super) fn update(
        &mut self,
        r: &Term,
        l: &str,
        v: &Term,
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let ft = field(t, l)?;
        let (rt, ftp) = (self.ty(t)?, self.ty(ft)?);
        let pr = self.typing(r, t, col + 2)?;
        let pv = self.typing(v, ft, col + 2)?;
        Ok(format!(
            "(.update (R := {rt}) (t := {ftp}){pad}{pr}{pad}rfl{pad}{pv})"
        ))
    }

    /// (Dproj) of field `l` of the dictionary `d`, at the type the check
    /// gave `e`: a method of no variables of its own, or a superclass.
    pub(super) fn dproj(
        &mut self,
        e: &Term,
        d: &Term,
        l: &str,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let at = self.inferred(d)?;
        let Type::Dict(c, args) = bare(&at) else {
            return Err("a method of no dictionary".into());
        };
        let class = (self.sig.classes.iter().position(|k| *k == c.to_string()))
            .ok_or_else(|| format!("the class `{c}`, not in the signature"))?;
        let us = args
            .iter()
            .map(|a| self.ty(a))
            .collect::<Result<Vec<_>, _>>()?;
        let method = self.ty(&self.inferred(e)?)?;
        let pd = self.typing(d, &at, col + 2)?;
        Ok(format!(
            "(Cert.typ_dproj (c := {class}) (us := [{}]) (l := {l:?}) (T' := {method}){pad}{pd}{pad}((hS.dict _ rfl).trans rfl) rfl rfl (by inst_eq) (by le_live))",
            us.join(", ")
        ))
    }

    /// (Rec): the fields by label, each at its type. Out-of-order records with
    /// non-value expressions bind fields in source order via `.let_` before
    /// constructing the canonical `.rcons` spine.
    pub(super) fn record(
        &mut self,
        fs: &[(Name, Term)],
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let f = fields(t).ok_or("a record at no record type")?;
        if needs_source_lets(fs) {
            return self.record_let(fs, f, 0, col);
        }
        let pad = format!("\n{}", " ".repeat(col + 2));
        let mut proof = ".rnil".to_owned();
        let mut rest = Fields {
            fields: BTreeMap::new(),
            tail: None,
        };
        for (l, x) in sorted(fs).into_iter().rev() {
            let ft = f.fields.get(l).ok_or("a field the type does not have")?;
            let px = self.typing(x, ft, col + 2)?;
            let r = self.ty(&Type::Record(rest.clone()))?;
            proof = format!("(.rcons (R := {r}) trivial (by decide){pad}{px}{pad}{proof})");
            rest.fields.insert(l.clone(), ft.clone());
        }
        Ok(proof)
    }

    fn record_let(
        &mut self,
        fs: &[(Name, Term)],
        f: &Fields,
        idx: usize,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        if idx < fs.len() {
            let (l, x) = &fs[idx];
            let ft = f.fields.get(l).ok_or("a field the type does not have")?;
            let a = self.ty(ft)?;
            let px = self.typing(x, ft, col + 2)?;
            let fresh = Name::from(format!("$rec_{idx}"));
            let rest = self.under(&fresh, |c| c.record_let(fs, f, idx + 1, col + 2))?;
            return Ok(format!("(.let_ (a := {a}){pad}{px}{pad}{rest})"));
        }
        let mut indexed: Vec<(usize, &Name)> =
            fs.iter().enumerate().map(|(i, (l, _))| (i, l)).collect();
        indexed.sort_by(|a, b| a.1.encode_utf16().cmp(b.1.encode_utf16()));
        let mut proof = ".rnil".to_owned();
        let mut rest = Fields {
            fields: BTreeMap::new(),
            tail: None,
        };
        for (_, l) in indexed.into_iter().rev() {
            let ft = f.fields.get(l).ok_or("a field the type does not have")?;
            let ft_ty = self.ty(ft)?;
            let px = format!(
                "(Cert.typ_var (ts := []) (T' := {ft_ty}) rfl rfl (by holds) (by inst_le) (by le_live))"
            );
            let r = self.ty(&Type::Record(rest.clone()))?;
            proof = format!("(.rcons (R := {r}) trivial (by decide){pad}{px}{pad}{proof})");
            rest.fields.insert(l.clone(), ft.clone());
        }
        Ok(proof)
    }
}
