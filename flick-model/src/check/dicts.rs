//! Dictionaries before elaboration (`docs/formal/core.md` §1.6): the
//! fields of `S.Impl` and `C.Dict τ̄` that (Field-Impl) instantiates, and
//! records checked as either.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use super::expr::mono;
use super::generalize::{Inst, params, substitute, substitute_caps};
use super::{Check, Checker, error};
use crate::syntax::{Name, Path, Term};
use crate::types::{Fields, Kind, Row, Scheme, Type, Var};

/// An operation of `S τ̄`: the first variables of its scheme are the
/// service's parameters, here `τ̄`; a row argument is `Errors ρ`, as
/// instantiating gives it.
pub(super) fn at_args(op: &Scheme, args: &[Type]) -> Scheme {
    let n = args.len().min(op.vars.len());
    if n == 0 {
        return op.clone();
    }
    let map: HashMap<Name, Inst> = op.vars[..n]
        .iter()
        .map(|(v, _)| v.clone())
        .zip(args.iter().map(|a| match a {
            Type::Errors(r) => Inst::Row(r.clone()),
            t => Inst::Type(t.clone()),
        }))
        .collect();
    Scheme {
        vars: op.vars[n..].to_vec(),
        body: substitute(&op.body, &map),
        outlives: op
            .outlives
            .iter()
            .map(|(c, r)| (substitute_caps(c, &map), substitute(r, &map)))
            .collect(),
    }
}

impl Checker<'_> {
    /// `∀p̄. S p̄`: service `s` at its parameters, of the kinds its first
    /// operation gives them.
    pub(super) fn service_row(&self, s: &Path) -> (Vec<(Name, Kind)>, Row) {
        let n = self.decls.service_params.get(s).copied().unwrap_or(0);
        let first = self.decls.services.get(s).and_then(|ops| ops.first());
        let vars: Vec<(Name, Kind)> = (0..n)
            .map(|i| match first.and_then(|(_, op)| op.vars.get(i)) {
                Some(v) => v.clone(),
                None => (Name::from(format!("p{i}").as_str()), Kind::Type),
            })
            .collect();
        let args: Rc<[Type]> = vars
            .iter()
            .map(|(v, _)| Type::Var(Var::Named(v.clone())))
            .collect();
        let row = Row {
            elems: [(s.clone(), args)].into(),
            vars: BTreeSet::new(),
        };
        (vars, row)
    }

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
        let args = self.zonk_row(r).elems[&s].clone();
        let ops = self
            .decls
            .services
            .get(&s)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let fields = ops
            .iter()
            .map(|(l, op)| {
                let op = at_args(op, &args);
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
