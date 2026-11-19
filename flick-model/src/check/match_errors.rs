//! (Match-Err) and (Errors-In) of `docs/formal/core.md`: a `match` takes a
//! value of `Errors ε` apart by the errors of `ε`, and a term of an error
//! `E` has the type of the errors of a row that holds `E`. A pattern of an
//! error matches the element `E ῡ` of the row, the elements are the
//! constructors `Exh` splits, and a row not known yet holds the errors its
//! patterns are headed by. The match stays pure and removes no error from
//! a row.

use std::rc::Rc;

use super::unify::error_type;
use super::{Check, Checker, error};
use crate::syntax::{Arm, Con, Pat, Path, Term};
use crate::types::{Row, Type, Var};

impl Checker<'_> {
    /// What a pattern of `c` matches in a value of `t`: for `Errors ε` and
    /// an error `E`, the element `E ῡ` of `ε`. An error outside a row that
    /// is known is no pattern of it (`FLICK-T003`). A pattern of a `match`
    /// adds its error to a row not known yet, which stays open behind it
    /// until [`Self::close_error_rows`]; elsewhere the row takes the error
    /// as `Errors ρ ≡ E` does.
    pub(super) fn error_element(&mut self, c: &Con, t: &Type) -> Check<Type> {
        let Type::Errors(r) = self.zonk(&self.resolve(t)) else {
            return Ok(t.clone());
        };
        if !self.decls.errors.contains(&c.ty) {
            return Ok(t.clone());
        }
        let r = self.zonk_row(&r);
        if let Some(args) = r.elems.get(&c.ty) {
            return Ok(error_type(&c.ty, args));
        }
        let meta = r.vars.iter().find(|v| matches!(v, Var::Meta(_))).cloned();
        match meta {
            Some(_) if self.error_rows.is_none() => Ok(t.clone()),
            Some(v) => {
                let (name, args) = self.fresh_error(c)?;
                let rest = self.fresh_row();
                let mut added = rest.clone();
                added.elems.insert(name.clone(), args.clone());
                self.unify_rows(&Row::var(v), &added)?;
                let tails = self.error_rows.get_or_insert_default();
                tails.extend(rest.vars);
                Ok(error_type(&name, &args))
            }
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

    /// After the arms of a `match` on `t`: the rows its patterns added
    /// errors to stay open when the arms without `when` cover `t` with them
    /// open; otherwise `Exh` needs them closed, and the errors named are
    /// their only solution.
    pub(super) fn close_error_rows(
        &mut self,
        rows: Vec<Var>,
        t: &Type,
        unguarded: &[&Pat],
    ) -> Check<()> {
        if rows.is_empty() || self.exhaustive(t, unguarded)? {
            return Ok(());
        }
        for v in rows {
            self.close_row(&Row::var(v))?;
        }
        Ok(())
    }

    /// The unknown variables of `r` become the empty row.
    fn close_row(&mut self, r: &Row) -> Check<()> {
        let open: Vec<Var> = self
            .zonk_row(r)
            .vars
            .into_iter()
            .filter(|v| matches!(v, Var::Meta(_)))
            .collect();
        for v in open {
            self.unify_rows(&Row::var(v), &Row::none())?;
        }
        Ok(())
    }

    /// The row of an arm `x` or `_` of a `try`: a row not known yet, which
    /// the arm's uses of `x` and the patterns of a `match` on it tell. Once
    /// the arms are checked it holds what the body raises (`raised ⊆
    /// caught`), as (Try) with (Sub) types the body at a row above its own;
    /// what nothing asks of it is the least row.
    pub(super) fn caught_row(&mut self) -> Row {
        self.fresh_row()
    }

    /// (Errors-In): `e`, of type `t`, is checked against `expected`; for a
    /// term of an error `E` against `Errors ε` whose row names `E`, its type
    /// is the element `E ῡ` of `ε`. Else (Sub). A row of one error is that
    /// error, and a row not known yet takes it, as `Errors ρ ≡ E` does.
    pub(super) fn fit(&mut self, e: &Term, t: &Type, expected: &Type) -> Check<()> {
        let Type::Errors(r) = self.zonk(&self.resolve(expected)) else {
            return self.subsume(t, expected);
        };
        // an error holds no region, so its qualifier has nothing to keep
        let bare = self.resolve(t);
        let Some((name, _)) = self.error_of(&bare) else {
            return self.subsume(t, expected);
        };
        let r = self.zonk_row(&r);
        let Some(args) = r.elems.get(&name) else {
            let unknown = r.vars.iter().any(|v| matches!(v, Var::Meta(_)));
            if unknown || r.elems.is_empty() || (r.vars.is_empty() && r.elems.len() < 2) {
                return self.subsume(t, expected);
            }
            return error(
                "Errors-In",
                format!(
                    "`{name}` is not an error of `{}` (`FLICK-T003`)",
                    Type::Errors(r)
                ),
            );
        };
        let element = error_type(&name, args);
        self.unify(&bare, &element)?;
        self.note(e, |n| n.errors_in = Some(element));
        Ok(())
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

    /// The scrutinee of a `match` in a configuration: once the arm of a
    /// `try` has run, the `x` it took may have become the error raised, a
    /// value `E w̄` of its error type, which (Errors-In) types at a row of
    /// `E` and the errors the arms name; the arms are checked at it.
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
        Ok(t.clone())
    }

    /// The errors `p` is headed by, each at fresh arguments.
    fn error_heads(&mut self, p: &Pat, heads: &mut Row) -> Check<()> {
        match p {
            Pat::Or(ps) => ps.iter().try_for_each(|p| self.error_heads(p, heads)),
            Pat::Con(c, _) if self.decls.errors.contains(&c.ty) => {
                if !heads.elems.contains_key(&c.ty) {
                    let (e, args) = self.fresh_error(c)?;
                    heads.elems.insert(e, args);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The error `c` builds, at fresh arguments.
    fn fresh_error(&mut self, c: &Con) -> Check<(Path, Rc<[Type]>)> {
        let mut t = self.constructor(&c.ty.module, &c.name)?;
        while let Type::Fun(_, _, r) = t {
            t = (*r).clone();
        }
        self.error_of(&t).map_or_else(
            || error("Pattern", format!("`{}` builds no error", c.name)),
            Ok,
        )
    }
}
