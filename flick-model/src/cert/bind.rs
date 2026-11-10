//! The rules that bind in a certificate: (Let-Mono), (Let-Gen) and the
//! (Fix) of a `let rec`.

use crate::check::Inst;
use crate::syntax::{Name, Term};
use crate::types::{Type, Var};

use super::Unsupported;
use super::term::{Cx, arrow, index};

impl Cx<'_> {
    /// (Let-Mono) or, for a scheme with variables, (Let-Gen).
    pub(super) fn let_(
        &mut self,
        x: &Name,
        e1: &Term,
        e2: &Term,
        t: &Type,
        rec: bool,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let node = self.node(e1)?;
        let scheme = node.scheme.clone().ok_or("a `let` without its scheme")?;
        if !scheme.outlives.is_empty() {
            return Err("a scheme with facts".into());
        }
        let k = scheme.vars.len();
        let outer = self.env.clone();
        let under = |names: Vec<String>| -> Vec<String> {
            names
                .into_iter()
                .chain(outer.vars.iter().cloned())
                .collect()
        };
        self.env.vars = under(scheme.vars.iter().map(|(v, _)| v.to_string()).collect());
        let a = self.ty(&scheme.body);
        // a signed right side is typed at its rigid variables, the binders
        // of (Let-Gen)
        let rhs_ty = match (&node.rigid, &node.expected) {
            (Some(rigid), Some(t)) => {
                self.env.vars = under(rigid.iter().map(ToString::to_string).collect());
                t
            }
            _ => &scheme.body,
        };
        let rhs = if rec {
            self.fix(x, e1, rhs_ty, col + 2)
        } else {
            self.typing(e1, rhs_ty, col + 2)
        };
        self.env = outer;
        let (a, rhs) = (a?, rhs?);
        let rest = self.under(x, |c| c.typing(e2, t, col + 2))?;
        Ok(if k == 0 {
            format!("(.let_ (a := {a}){pad}{rhs}{pad}{rest})")
        } else {
            format!("(.letGen (k := {k}) (Φ := Facts.none) (a := {a}){pad}{rhs}{pad}{rest})")
        })
    }

    /// (Fix): the λ of a `let rec`, its body under its parameter and `f`.
    fn fix(&mut self, f: &Name, lam: &Term, t: &Type, col: usize) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let Term::Lam(x, body) = lam else {
            return Err("a `let rec` of no λ".into());
        };
        let (_, _, r, _) = arrow(t).ok_or_else(|| format!("a `let rec` at `{t}`"))?;
        let args = self.arrow_named(t, "r")?;
        let rigid = self.node(lam)?.rigid.clone().unwrap_or_default();
        self.fixed.push((self.terms.len(), rigid));
        let b = self.under(f, |c| c.under(x, |c| c.typing(body, r, col + 2)));
        self.fixed.pop();
        let b = b?;
        Ok(format!(
            "(.fix {args}{pad}{b}{pad}(by simp [Tm.NoIns, Tm.Has]) (by le_live))"
        ))
    }

    /// (Var) at the instance the check took. A `fix` binds itself at one
    /// type: in its body the check may take it only at its own rigid
    /// variables, the identity.
    pub(super) fn var(&mut self, e: &Term, x: &str) -> Result<String, Unsupported> {
        let at = self.terms.len() - 1 - index(&self.terms, x)?;
        let node = self.node(e)?;
        let (scheme, insts) = node
            .instance
            .as_ref()
            .ok_or_else(|| format!("`{x}` without its instance"))?;
        if !scheme.outlives.is_empty() {
            return Err("a scheme with facts".into());
        }
        let ts = match self.fixed.iter().find(|(p, _)| *p == at) {
            Some((_, rigid)) => {
                let own = insts.len() == rigid.len()
                    && insts
                        .iter()
                        .zip(rigid)
                        .all(|(i, v)| matches!(i, Inst::Type(Type::Var(Var::Named(n))) if n == v));
                if !own {
                    return Err("polymorphic recursion".into());
                }
                Vec::new()
            }
            None => insts
                .iter()
                .map(|i| self.inst(i))
                .collect::<Result<Vec<_>, _>>()?,
        };
        let instance = self.ty(&self.inferred(e)?)?;
        Ok(format!(
            "(Cert.typ_var (ts := [{}]) (T' := {instance}) rfl rfl (Cert.holds_none _) (by inst_eq) (by le_live))",
            ts.join(", ")
        ))
    }
}
