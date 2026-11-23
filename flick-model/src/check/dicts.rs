//! Dictionaries before elaboration (`docs/formal/core.md` §1.6): the
//! fields of `S.Impl` and `C.Dict τ̄` that (Field-Impl) instantiates, and
//! records checked as either.

use std::collections::HashMap;
use std::rc::Rc;

use super::expr::mono;
use super::generalize::{Inst, substitute, substitute_caps};
use super::{Check, Checker, error};
use crate::syntax::{GETTER, Name, Path, Term};
use crate::types::{Fields, Kind, Row, Scheme, Type, Var};

/// An operation of `S τ̄`: the first variables of its scheme are the
/// service's parameters, here `τ̄`; a row argument is `Errors ρ`, as
/// instantiating gives it.
/// An argument of a type as what it instantiates: a row written there is
/// `Errors ρ`.
pub(super) fn arg(a: &Type) -> Inst {
    match a {
        Type::Errors(r) => Inst::Row(r.clone()),
        Type::Caps(c) => Inst::Caps(c.clone()),
        t => Inst::Type(t.clone()),
    }
}

pub(super) fn at_args(op: &Scheme, args: &[Type]) -> Scheme {
    let n = args.len().min(op.vars.len());
    if n == 0 {
        return op.clone();
    }
    let map: HashMap<Name, Inst> = op.vars[..n]
        .iter()
        .map(|(v, _)| v.clone())
        .zip(args.iter().map(arg))
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
            ..Row::default()
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
        let map = class
            .params
            .iter()
            .cloned()
            .zip(args.iter().map(arg))
            .collect();
        if let Some(sup) = l.strip_prefix('$') {
            return match (0..class.supers.len())
                .find(|&i| class.super_slot(i).as_deref() == Some(sup))
            {
                Some(i) => Ok(mono(substitute(&class.supers[i], &map))),
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
            match x {
                // the field of a default is its getter's body
                Term::Lam(get, body) if &**get == GETTER => self.check_forall(body, &s)?,
                _ => self.check_forall(x, &s)?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decls::{Class, Decls};
    use crate::types::Stage;

    #[test]
    fn dictionary_method_uses_its_service_and_error_arguments() {
        let class = Path::new("Test", "UsesRows");
        let r = Name::from("r");
        let e = Name::from("e");
        let unit = Type::con("Prelude", "Unit");
        let mut decls = Decls::default();
        decls.classes.insert(
            class.clone(),
            Class {
                params: vec![r.clone(), e.clone()],
                ..Class::default()
            },
        );
        let services = Row::var(Var::Named(r.clone()));
        let errors = Row::var(Var::Named(e.clone()));
        let dict = Type::Dict(
            class.clone(),
            Rc::from([Type::Errors(services.clone()), Type::Errors(errors.clone())]),
        );
        let method = Type::fun(unit.clone(), Stage::Runtime(services, errors), unit.clone());
        decls.values.insert(
            Path::new("Test", "call"),
            Scheme::new(
                vec![(r, Kind::Services), (e, Kind::Errors)],
                Type::fun(dict, Stage::Pure, method),
            ),
        );
        let services = Row::var(Var::Named("services".into()));
        let errors = Row::var(Var::Named("errors".into()));
        let actual = Checker::new(&decls)
            .method(
                &class,
                &[Type::Errors(services.clone()), Type::Errors(errors.clone())],
                "call",
            )
            .expect("dictionary method");
        assert_eq!(
            actual.body,
            Type::fun(unit.clone(), Stage::Runtime(services, errors), unit)
        );
    }
}
