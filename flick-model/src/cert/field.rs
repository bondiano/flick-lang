//! Fields in a certificate: (Proj) and (Update) of a record, (Rec) of a
//! record literal, and (Dproj) of a dictionary, a method or a superclass.

use std::collections::BTreeMap;

use crate::syntax::{Name, Term};
use crate::types::{Fields, Type};

use super::Unsupported;
use super::sig::bare;
use super::term::{Cx, field, fields, sorted};

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

    /// (Rec): the fields by label, each at its type.
    pub(super) fn record(
        &mut self,
        fs: &[(Name, Term)],
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let f = fields(t).ok_or("a record at no record type")?;
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
}
