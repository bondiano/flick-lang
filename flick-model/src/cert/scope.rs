//! `scope s { e }` in a certificate: (Scope) binds the region as type
//! variable 0 and its owner as term variable 0, with no facts of its own.

use std::iter;

use crate::syntax::{Name, Term};
use crate::translate::lean::elide::{row, stage};
use crate::types::{Stage, Type};

use super::Unsupported;
use super::sig::Sig;
use super::term::{Cx, tm};

/// `scope s { e }` in the core: `s` the owner, innermost.
pub(super) fn scope_tm(
    s: &Name,
    body: &Term,
    ctx: &[Name],
    sig: &Sig,
) -> Result<String, Unsupported> {
    let inner: Vec<Name> = iter::once(s.clone()).chain(ctx.iter().cloned()).collect();
    Ok(format!("(.scope {})", tm(body, &inner, sig)?))
}

impl Cx<'_> {
    /// (Scope): the body under the region and its owner, on its own stage;
    /// what it passes out is the row the check leaves outside.
    pub(super) fn scope(
        &mut self,
        s: &Name,
        body: &Term,
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let node = self.node(body)?;
        let (own, out) = node.own.clone().ok_or("a `scope` without its stage")?;
        let region = node.region.clone().ok_or("a `scope` without its region")?;
        let errors = match &own {
            Stage::Pure => "Row.none".to_owned(),
            Stage::Runtime(_, e) => row(e, &self.env)?,
        };
        let (outside, r) = (self.ty(t)?, row(&out, &self.env)?);
        let env = self.env.clone();
        self.env.vars.insert(0, region.to_string());
        let inside = self.ty(t);
        let phi = stage(&own, &self.env);
        let pb = self.under(s, |c| c.typing(body, t, col + 2));
        self.env = env;
        let (inside, phi, pb) = (inside?, phi?, pb?);
        Ok(format!(
            "(Cert.typ_scope (F := Facts.none) (φ' := {phi}) (r := {r}) (ε := {errors}) (t := {outside}) (t' := {inside}){pad}(by simp [Tm.NoIns, Tm.Has]) (fun _ _ h => h.elim){pad}{pb}{pad}(by inst_eq) (by stage_le) (by stage_le) (by le_live))"
        ))
    }
}
