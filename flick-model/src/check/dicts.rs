//! Dictionaries before elaboration (`docs/formal/core.md` §1.6): the
//! fields of `S.Impl` and `C.Dict τ̄` that (Field-Impl) instantiates, and
//! records checked as either.

use std::rc::Rc;

use super::expr::mono;
use super::generalize::{params, substitute};
use super::{Check, Checker, error};
use crate::syntax::{Name, Path, Term};
use crate::types::{Fields, Row, Scheme, Type};

impl Checker<'_> {
    /// The one service of `S.Impl`.
    pub(super) fn service(&self, r: &Row) -> Check<Path> {
        let r = self.zonk_row(r);
        let services: Vec<_> = r.elems.keys().collect();
        match (services.as_slice(), r.vars.is_empty()) {
            ([s], true) => Ok((*s).clone()),
            _ => error(
                "Field",
                format!("`ServiceImpl {r}` is not the implementation of one service"),
            ),
        }
    }

    /// The type of field `l` of `C.Dict τ̄`: a method with the class's
    /// parameters as `τ̄`, or `$S`, the dictionary of superclass `S`.
    pub(super) fn method(&mut self, c: &Path, args: &[Type], l: &str) -> Check<Scheme> {
        let Some(class) = self.decls.classes.get(c).cloned() else {
            return error("Field", format!("no class `{c}`"));
        };
        let map = params(&class.params, args);
        if let Some(sup) = l.strip_prefix('$') {
            return match class
                .supers
                .iter()
                .find(|s| matches!(s, Type::Dict(p, _) if &*p.name == sup))
            {
                Some(s) => Ok(mono(substitute(s, &map))),
                None => error("Field", format!("`{}` has no superclass `{sup}`", c.name)),
            };
        }
        match self.decls.method(c, l) {
            Some((_, s)) => Ok(Scheme {
                vars: s.vars.clone(),
                body: substitute(&s.body, &map),
                outlives: Vec::new(),
            }),
            None => error("Field", format!("`{}` has no method `{l}`", c.name)),
        }
    }

    /// `S.Impl` as the closed record of the operations of `S`, a field of
    /// a polymorphic one a `∀`: flick-core's alias `S.Impl` (§1.6).
    pub(super) fn implementation(&self, r: &Row) -> Check<Fields> {
        let s = self.service(r)?;
        let ops = self
            .decls
            .services
            .get(&s)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let fields = ops
            .iter()
            .map(|(l, op)| {
                let t = if op.vars.is_empty() {
                    op.body.clone()
                } else {
                    Type::Forall(Rc::new(op.clone()))
                };
                (l.clone(), t)
            })
            .collect();
        Ok(Fields { fields, tail: None })
    }

    /// A record checked as `C.Dict τ̄`: each method and superclass.
    pub(super) fn dictionary(
        &mut self,
        fields: &[(Name, Term)],
        c: &Path,
        args: &[Type],
    ) -> Check<()> {
        let methods = self
            .decls
            .classes
            .get(c)
            .map(|k| k.methods.clone())
            .unwrap_or_default();
        if let Some(m) = methods
            .iter()
            .find(|m| !fields.iter().any(|(l, _)| l == *m))
        {
            return error(
                "Rec",
                format!("the dictionary of `{}` has no `{m}`", c.name),
            );
        }
        for (l, x) in fields {
            let s = self.method(c, args, l)?;
            self.check_forall(x, &s)?;
        }
        Ok(())
    }
}
