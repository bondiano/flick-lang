//! Instantiating and skolemizing checked schemes.

use crate::source::Span;
use crate::types::{Checker, Elem, ty::Type, typing::passes_dictionary};
use std::collections::HashSet;

impl Checker<'_> {
    /// Records the type variable elements of error rows in `t`, as the
    /// signature wrote them (`~t`), to check that each is an error type.
    fn error_elements(&mut self, t: &Type, span: Span) {
        let mut found = Vec::new();
        t.visit(&mut |t| {
            if let Type::Row(row) = t {
                let elements = row
                    .entries
                    .iter()
                    .filter(|(label, _)| matches!(label, Elem::Error(_)));
                found.extend(elements.map(|(_, element)| (element.clone(), span)));
            }
        });
        self.cx.regions.error_elements.extend(found);
    }

    pub fn instantiate(&mut self, t: &Type, span: Span) -> Type {
        match self.store.resolve(t) {
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Type::Captured {
                ty: Box::new(self.instantiate(&ty, span)),
                captures,
                value,
                elided,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.instantiate(&ty, span)),
                captures,
            },
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| (v.name.clone(), self.store.fresh_of(&v.kind)))
                    .collect();
                for c in &constraints {
                    self.want(c.substitute(&subst), span);
                }
                let body = body.substitute(&subst);
                self.error_elements(&body, span);
                self.instantiate(&body, span)
            }
            other => other,
        }
    }

    /// The name of the rigid variable for `name` bound with `constraints`:
    /// numbered when an assumption in scope that passes a dictionary uses it
    /// too, since code generation finds a dictionary by its constraint as shown.
    fn unshadowed(&self, name: &str, constraints: &[Type]) -> String {
        let is_var = |t: &Type| matches!(t, Type::Var(n) if n == name);
        if !constraints
            .iter()
            .any(|c| passes_dictionary(c) && c.contains(&mut |t| is_var(t)))
        {
            return name.to_owned();
        }
        let mut used = HashSet::new();
        for g in self.classes.givens.iter().filter(|g| passes_dictionary(g)) {
            g.visit(&mut |t| match t {
                Type::Var(n) => drop(used.insert(n.clone())),
                Type::Skolem(s) => drop(used.insert(self.store.skolem_name(*s).to_owned())),
                _ => {}
            });
        }
        if !used.contains(name) {
            return name.to_owned();
        }
        // `used` is finite: one of its length + 1 numbers is free
        (1..=used.len() + 1)
            .map(|i| format!("{name}{i}"))
            .find(|n| !used.contains(n))
            .expect("a free number")
    }

    /// The body of a polymorphic type with fresh skolems at the current
    /// level, the skolems by name, and its class constraints over them.
    pub fn skolemize(&mut self, t: &Type) -> (Type, Vec<(String, Type)>, Vec<Type>) {
        match self.store.resolve(t) {
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| {
                        let ty = self.store.skolem(&self.unshadowed(&v.name, &constraints));
                        if let Type::Skolem(s) = &ty {
                            self.cx.regions.rigid_kinds.push((*s, v.kind.clone()));
                        }
                        (v.name.clone(), ty)
                    })
                    .collect();
                let (body, mut more, mut givens) = self.skolemize(&body.substitute(&subst));
                more.splice(0..0, subst.iter().cloned());
                givens.splice(0..0, constraints.iter().map(|c| c.substitute(&subst)));
                (body, more, givens)
            }
            other => (other, Vec::new(), Vec::new()),
        }
    }
}
