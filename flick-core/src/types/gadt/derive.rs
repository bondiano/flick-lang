//! The fields of a constructor of a GADT as a derived instance of its type
//! sees them: over the type's parameters, under what the constructor
//! builds. One that hides a type or holds a dictionary has no derived
//! instance: what its fields are depends on the value.

use super::columns::replace_skolems;
use super::refine::Refined;
use super::scheme::{built, is_hidden_captures};
use crate::types::Checker;
use crate::types::ty::{Binder, Type};

impl Checker<'_> {
    /// The fields of the constructor with `scheme`, of the type `name` of
    /// this module with `params`, over those parameters; `Err` is why a
    /// derived instance cannot cover it.
    pub(in crate::types) fn indexed_fields(
        &mut self,
        scheme: &Type,
        name: &str,
        params: &[Binder],
    ) -> Result<Vec<Type>, String> {
        let (vars, constraints, body) = match scheme {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (&vars[..], &constraints[..], &**body),
            t => (&[][..], &[][..], t),
        };
        if let Some(c) = constraints.first() {
            return Err(format!("holds the dictionary of `{}`", self.show(c)));
        }
        let result = built(body);
        let locals: Vec<String> = vars
            .iter()
            .filter(|v| !is_hidden_captures(v))
            .map(|v| v.name.clone())
            .collect();
        if let Some(hidden) = locals
            .iter()
            .find(|v| !result.contains(&mut |t| matches!(t, Type::Var(x) if x == *v)))
        {
            return Err(format!("hides the type `{hidden}` of a field"));
        }
        // the parameters as rigid variables, which the result may refine
        let rigid: Vec<Type> = params.iter().map(|p| self.store.skolem(&p.name)).collect();
        let own = Type::apps(Type::con(&self.env.module, name), rigid.iter().cloned());
        let Refined::Unifier { bound, equals } = self.refine(&locals, result, &own) else {
            return Err("builds an instance that a derived instance does not cover".to_owned());
        };
        // what the result leaves open stays a variable of the constructor's own
        let subst: Vec<(String, Type)> = locals
            .iter()
            .map(|v| match bound.iter().find(|(b, _)| b == v) {
                Some((_, t)) => (v.clone(), t.clone()),
                None => (v.clone(), Type::Var(format!("{v}'"))),
            })
            .collect();
        let back: Vec<(u32, Type)> = rigid
            .iter()
            .zip(params)
            .filter_map(|(r, p)| match r {
                Type::Skolem(s) => Some((*s, Type::Var(p.name.clone()))),
                _ => None,
            })
            .collect();
        let mut fields = Vec::new();
        let mut current = body.substitute(&subst);
        while let Type::Fun { param, result, .. } = current {
            // a parameter the constructor fixes is what it fixes it to
            let field = replace_skolems(&replace_skolems(&param, &equals), &back);
            fields.push(field.substitute(&subst));
            current = *result;
        }
        Ok(fields)
    }
}

