//! A list literal in a certificate: the core has no list, so `[x, y]` is the
//! spine `Cons x (Cons y Nil)` of constructors only the signature has.

use crate::syntax::{Name, Term};
use crate::types::Type;

use super::Unsupported;
use super::sig::{Sig, bare};
use super::term::{Cx, tm};

/// `[x₁, …, xₙ]` in the core.
pub(super) fn list_tm(xs: &[Term], ctx: &[Name], sig: &Sig) -> Result<String, Unsupported> {
    xs.iter()
        .rev()
        .try_fold("(.ctor \"Prelude.Nil\")".to_owned(), |rest, x| {
            Ok(format!(
                "(.app (.app (.ctor \"Prelude.Cons\") {}) {rest})",
                tm(x, ctx, sig)?
            ))
        })
}

/// The element type of `List a`.
fn element(t: &Type) -> Option<&Type> {
    match bare(t) {
        Type::App(h, a) if matches!(&**h, Type::Con(p) if p.to_string() == "Prelude.List") => {
            Some(a)
        }
        _ => None,
    }
}

impl Cx<'_> {
    /// (Con) of `Nil` and of `Cons` applied to each element at `a`.
    pub(super) fn list(
        &mut self,
        xs: &[Term],
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let a = element(t).ok_or_else(|| format!("a list at `{t}`"))?;
        let (at, lt) = (self.ty(a)?, self.ty(t)?);
        let ctor = |k: &str| {
            format!(
                "(Typ.ctor_plain S (K := \"Prelude.{k}\") (us := [{at}]) ((hS.ctor _ rfl).trans rfl) ((hS.res _ rfl).trans rfl) rfl LeLive.refl)"
            )
        };
        let mut proof = ctor("Nil");
        for x in xs.iter().rev() {
            let px = self.typing(x, a, col + 2)?;
            let partial = format!("(.arrow {lt} .pure {lt} Caps.none)");
            proof = format!(
                "(.app (a := {lt}) (ψ := .pure) (t := {lt}) (C := Caps.none){pad}(.app (a := {at}) (ψ := .pure) (t := {partial}) (C := Caps.none){pad}{}{pad}{px}{pad}(by stage_le) (by le_live)){pad}{proof}{pad}(by stage_le) (by le_live))",
                ctor("Cons")
            );
        }
        Ok(proof)
    }
}
