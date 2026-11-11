//! `raise` and `try` in a certificate: (Raise-Adt) of a value of an error
//! type, and (Try) with the errors each arm catches (`Catches` of
//! `formal/Formal/Typing.lean`) by the lemmas of
//! `formal/FormalCert/Match.lean`.

use crate::syntax::{Arm, Const, Head, Pat, Term};
use crate::translate::lean::elide::row;
use crate::types::{Stage, Type};

use super::Unsupported;
use super::pat::pat;
use super::sig::bare;
use super::term::Cx;

/// `f` is `raise`.
pub(super) fn is_raise(f: &Term) -> bool {
    matches!(f, Term::Apply(Head::Const(Const::Raise), xs) if xs.is_empty())
}

impl Cx<'_> {
    /// (Raise-Adt): the value raised is of the error type `E`, the `raise`
    /// of any type `t`.
    pub(super) fn raise(&mut self, a: &Term, t: &Type, col: usize) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let u = self.inferred(a)?;
        let Type::Con(e) = bare(&u) else {
            return Err(format!("a raise of `{u}`"));
        };
        let pa = self.typing(a, &u, col + 2)?;
        Ok(format!(
            "(.raiseAdt (U := {}) (E := \"{e}\") (t := {}){pad}{pa}{pad}rfl (by stage_le))",
            self.ty(&u)?,
            self.ty(t)?
        ))
    }

    /// (Try) of a body that raises `ε₀` on its own stage, or (Try-Pure):
    /// from the last arm up, `H` is what the arms catch, all of `ε₀` after
    /// `x` or `_`, and `E` after an arm `E x̄`; what they do not passes on.
    pub(super) fn try_(
        &mut self,
        body: &Term,
        arms: &[Arm],
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let (stage, rest) = self
            .node(body)?
            .own
            .clone()
            .ok_or("a `try` without its stage")?;
        let pb = self.typing(body, t, col + 2)?;
        let (services, e0) = match &stage {
            Stage::Pure => (None, "Row.none".to_owned()),
            Stage::Runtime(r, e) => (Some(row(r, &self.env)?), row(e, &self.env)?),
        };
        let boolean = Type::con("Prelude", "Bool");
        let mut h = "Row.none".to_owned();
        let mut alts = "(.hnil (fun _ h => h))".to_owned();
        for arm in arms.iter().rev() {
            let (_, names) = pat(&arm.pat)?;
            let b = self.under_all(&names, |c| c.typing(&arm.body, t, col + 2))?;
            // the pattern, and the error type of an arm `E p̄` with whether
            // its fields' patterns cover it
            let (hp, error) = match &arm.pat {
                Pat::Var(_) => (".var".to_owned(), None),
                Pat::Wild => (".wild".to_owned(), None),
                Pat::Con(c, ps) => {
                    let spine = self.spine_ty(&arm.pat, &Type::Con(c.ty.clone()))?;
                    let any = ps.iter().all(|p| matches!(p, Pat::Wild | Pat::Var(_)));
                    (format!("(.ctor {spine})"), Some((c.ty.to_string(), any)))
                }
                _ => return Err("a handler pattern of no error".into()),
            };
            let after = h.clone();
            alts = if let Some(g) = &arm.guard {
                let pg = self.under_all(&names, |c| c.typing(g, &boolean, col + 2))?;
                format!(
                    "(.hguard (H' := {after}) {hp}{pad}{pg}{pad}{b}{pad}{alts}{pad}(fun _ h => h))"
                )
            } else {
                let catches = match error {
                    None => {
                        h.clone_from(&e0);
                        "catches_all".to_owned()
                    }
                    Some((e, true)) => {
                        h = format!("(fun x => x = .nm {e:?} ∨ {after} x)");
                        format!(
                            "(catches_ctor (exh_gen hS (T := {e:?}) (us := []) (by decide) (by decide)))"
                        )
                    }
                    Some((_, false)) => "(fun _ h => .inr h)".to_owned(),
                };
                format!("(.hcons (H' := {after}) {hp}{pad}{b}{pad}{alts}{pad}{catches})")
            };
        }
        Ok(match services {
            None => format!("(.tryP (H := {h}){pad}{pb}{pad}{alts})"),
            Some(r) => format!(
                "(.tryR (r := {r}) (ε₀ := {e0}) (H := {h}) (ε' := {}){pad}{pb}{pad}{alts}{pad}(by stage_le) (by row_set))",
                row(&rest, &self.env)?
            ),
        })
    }
}
