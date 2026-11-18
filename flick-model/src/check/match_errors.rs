//! (Match-Err) of `docs/formal/core.md`: a `match` takes a value of
//! `Errors ε` apart by the errors of `ε`. A pattern of an error `E`
//! matches the element `E ῡ` of the row, the elements are the constructors
//! `Exh` splits, and a row not known yet is the errors the arms are headed
//! by. The match stays pure and removes no error from a row.

use std::rc::Rc;

use super::unify::error_type;
use super::{Check, Checker, error};
use crate::syntax::{Arm, Con, Pat, Term};
use crate::types::{Row, Type, Var};

impl Checker<'_> {
    /// What a pattern of `c` matches in a value of `t`: for `Errors ε` and
    /// an error `E`, the element `E ῡ` of `ε`. An error outside a row that
    /// is known is no pattern of it (`FLICK-T003`); a row not known yet
    /// takes it as `Errors ρ ≡ E` does.
    pub(super) fn error_element(&mut self, c: &Con, t: &Type) -> Check<Type> {
        let Type::Errors(r) = self.zonk(&self.resolve(t)) else {
            return Ok(t.clone());
        };
        if !self.decls.errors.contains(&c.ty) {
            return Ok(t.clone());
        }
        let r = self.zonk_row(&r);
        match r.elems.get(&c.ty) {
            Some(args) => Ok(error_type(&c.ty, args)),
            None if r.vars.iter().any(|v| matches!(v, Var::Meta(_))) => Ok(t.clone()),
            None => error(
                "Pattern",
                format!(
                    "`{}` is not an error of `{}` (`FLICK-T003`)",
                    c.ty,
                    Type::Errors(r)
                ),
            ),
        }
    }

    /// The row of an arm `x` or `_` of a `try` whose body raises `raised`:
    /// that row in a program. In a configuration the body may have stepped
    /// past what it raised, which (Try) still types at the row it had, as
    /// (Sub) lets it: the arm takes a row above, that a `match` on `x` may
    /// tell (`errors_of_arms`).
    pub(super) fn caught_row(&mut self, raised: &Row) -> Row {
        let mut row = raised.clone();
        if self.config {
            row.vars.extend(self.fresh_row().vars);
        }
        row
    }

    /// The constructors of the values of `Errors ε`: one for each error of
    /// a closed `ε`, `None` for a row that may hold others.
    pub(super) fn error_constructors(&self, r: &Row) -> Option<Vec<Rc<Con>>> {
        if !r.vars.is_empty() || !r.less.is_empty() {
            return None;
        }
        let arity = |e| {
            self.decls
                .variants
                .get(e)
                .and_then(|v| v.first())
                .map_or(0, |v| v.1)
        };
        Some(
            r.elems
                .keys()
                .map(|e| {
                    Rc::new(Con {
                        ty: e.clone(),
                        name: e.name.clone(),
                        arity: arity(e),
                        dicts: 0,
                    })
                })
                .collect(),
        )
    }

    /// `match e with ā` with `e : Errors ρ` and `ρ` not known yet: without
    /// an arm for every error, `ρ` is exactly the errors the arms are
    /// headed by and those it holds already, its only solution, and the
    /// arms are checked at that row. An arm for every error tells nothing
    /// of `ρ`, which then has to be known (`FLICK-T057`). Unknown parts are
    /// the variables of inference: a `try` arm `x` takes the row of its
    /// body, which calls may leave open here.
    ///
    /// In a configuration the `x` of the arm may have become the error
    /// raised, a value `E w̄`, which (Errors-Val) of the runtime forms types at a row of
    /// errors with `E`: the arms are checked at it with the errors they
    /// name.
    pub(super) fn errors_of_arms(&mut self, e: &Term, t: &Type, arms: &[Arm]) -> Check<Type> {
        let t0 = self.zonk(&self.resolve(t));
        if self.config
            && e.is_value()
            && let Some((name, args)) = self.error_of(&t0)
        {
            let mut heads = Row::none();
            for arm in arms {
                self.error_heads(&arm.pat, &mut heads)?;
            }
            if heads.elems.keys().any(|h| *h != name) {
                heads.elems.entry(name).or_insert(args);
                return Ok(Type::Errors(heads));
            }
        }
        let Type::Errors(r) = t0 else {
            return Ok(t.clone());
        };
        let r = self.zonk_row(&r);
        if r.vars.is_empty() || r.vars.iter().any(|v| !matches!(v, Var::Meta(_))) {
            return Ok(t.clone());
        }
        let (mut heads, mut every) = (Row::none(), false);
        for arm in arms {
            match &arm.pat {
                Pat::Var(_) | Pat::Wild => every |= arm.guard.is_none(),
                p => self.error_heads(p, &mut heads)?,
            }
        }
        if heads.elems.is_empty() {
            return Ok(t.clone());
        }
        if every {
            return error(
                "Match",
                "an arm for every error of a row not known yet (`FLICK-T057`)",
            );
        }
        for (e, args) in &r.elems {
            heads.elems.entry(e.clone()).or_insert_with(|| args.clone());
        }
        self.unify_rows(&r, &heads)?;
        Ok(Type::Errors(heads))
    }

    /// The errors `p` is headed by, each at fresh arguments.
    fn error_heads(&mut self, p: &Pat, heads: &mut Row) -> Check<()> {
        match p {
            Pat::Or(ps) => ps.iter().try_for_each(|p| self.error_heads(p, heads)),
            Pat::Con(c, _) if self.decls.errors.contains(&c.ty) => {
                if heads.elems.contains_key(&c.ty) {
                    return Ok(());
                }
                let mut t = self.constructor(&c.ty.module, &c.name)?;
                while let Type::Fun(_, _, r) = t {
                    t = (*r).clone();
                }
                if let Some((e, args)) = self.error_of(&t) {
                    heads.elems.insert(e, args);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
