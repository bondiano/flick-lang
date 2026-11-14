//! The types of the columns of the exhaustiveness check: the fields of a
//! constructor at the type of the value it is matched against, and whether
//! that value can be built by it at all.

use super::refine::Refined;
use super::row_facts::{self, Equation};
use super::scheme::{built, is_hidden_captures};
use crate::iface::{Entity, Namespace};
use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::patterns::Ctor;
use crate::types::ty::{Binder, Type};

/// What each rigid variable of a matched type equals, as a constructor tells.
type Equals = Vec<(u32, Type)>;

/// Facts learned along one usefulness specialization. Constructor-local row
/// variables have a fresh scope at each step; facts never enter inference.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::types) struct Coverage {
    equations: Vec<Equation>,
    scope: usize,
}

impl Coverage {
    pub(in crate::types) fn new(checker: &Checker<'_>) -> Self {
        Self {
            equations: checker.store.row_equalities.clone(),
            scope: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::types) struct Columns {
    pub types: Vec<Type>,
    pub facts: Coverage,
}

impl Checker<'_> {
    /// Whether the constructor `name` of the type `module.ty_name` builds
    /// values of type `ty`.
    pub(in crate::types) fn builds(
        &self,
        module: &str,
        ty_name: &str,
        name: &str,
        ty: &Type,
    ) -> bool {
        let ctor = Ctor::Variant {
            module: module.to_owned(),
            ty: ty_name.to_owned(),
            name: name.to_owned(),
        };
        self.columns_after(&ctor, ty, 0, &[], &Coverage::new(self))
            .is_some()
    }

    /// The columns once one of type `ty` is specialized to `ctor`: its
    /// `arity` fields, then `rest`, the other parts of the same value, which
    /// learn what `ctor` tells about the rigid variables of `ty`. `None` when
    /// no value of `ty` is built by `ctor`.
    pub(in crate::types) fn columns_after(
        &self,
        ctor: &Ctor,
        ty: &Type,
        arity: usize,
        rest: &[Type],
        facts: &Coverage,
    ) -> Option<Columns> {
        let (mut columns, equals, rows) = self.ctor_fields_at(ctor, ty, arity, facts.scope)?;
        columns.extend(rest.iter().map(|t| replace_skolems(t, &equals)));
        let mut equations: Vec<_> = facts
            .equations
            .iter()
            .map(|e| Equation {
                kind: e.kind,
                left: replace_skolems(&e.left, &equals),
                right: replace_skolems(&e.right, &equals),
                level: e.level,
            })
            .collect();
        equations.extend(rows);
        row_facts::consistent(self, &equations).then_some(Columns {
            types: columns,
            facts: Coverage {
                equations,
                scope: facts.scope + 1,
            },
        })
    }

    /// The types of the `arity` fields of `ctor` in a value of type `ty`, and
    /// what it tells about the rigid variables of `ty`; `None` when no value
    /// of `ty` is built by it. A type that is not known makes its fields not
    /// known either (`Error`).
    fn ctor_fields_at(
        &self,
        ctor: &Ctor,
        ty: &Type,
        arity: usize,
        scope: usize,
    ) -> Option<(Vec<Type>, Equals, Vec<Equation>)> {
        let unknown = || Some((vec![Type::Error; arity], Vec::new(), Vec::new()));
        if *ty == Type::Error {
            return unknown();
        }
        let t = self.whnf(ty);
        match ctor {
            Ctor::Variant { module, name, .. } => {
                let entity = Entity {
                    module: module.clone(),
                    namespace: Namespace::Constructor,
                    name: name.clone(),
                };
                let Some(scheme) = self.env.value(&entity) else {
                    return unknown();
                };
                let (vars, body) = match scheme {
                    Type::Forall { vars, body, .. } => (&vars[..], &**body),
                    t => (&[][..], t),
                };
                let fresh: Vec<_> = vars
                    .iter()
                    .filter(|v| !is_hidden_captures(v))
                    .map(|v| {
                        (
                            v.name.clone(),
                            Type::Var(format!("$coverage/{scope}/{}", v.name)),
                        )
                    })
                    .collect();
                let body = body.substitute(&fresh);
                let vars: Vec<_> = vars
                    .iter()
                    .map(|v| {
                        let name = fresh.iter().find(|(name, _)| name == &v.name).map_or_else(
                            || v.name.clone(),
                            |(_, ty)| {
                                let Type::Var(name) = ty else { unreachable!() };
                                name.clone()
                            },
                        );
                        Binder { name, ..v.clone() }
                    })
                    .collect();
                let locals: Vec<Binder> = vars
                    .iter()
                    .filter(|v| !is_hidden_captures(v))
                    .cloned()
                    .collect();
                let (bound, equals, rows) = match self.refine(&locals, built(&body), &t) {
                    Refined::Impossible => return None,
                    Refined::Unifier {
                        bound,
                        equals,
                        rows,
                    } => (bound, equals, rows),
                    Refined::Unknown | Refined::Unsupported(_) => return unknown(),
                };
                // a variable the constructor hides is not known to the check
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| {
                        let t = bound.iter().find(|(b, _)| *b == v.name);
                        (v.name.clone(), t.map_or(Type::Error, |(_, t)| t.clone()))
                    })
                    .collect();
                let mut fields = Vec::new();
                let mut current = body.substitute(&subst);
                while let Type::Fun { param, result, .. } = current {
                    fields.push(replace_skolems(&param, &equals));
                    current = *result;
                }
                if fields.len() == arity {
                    Some((fields, equals, rows))
                } else {
                    unknown()
                }
            }
            Ctor::Tuple(_) => match t {
                Type::Tuple(items) if items.len() == arity => Some((items, Vec::new(), Vec::new())),
                _ => unknown(),
            },
            Ctor::List(_) | Ctor::ListRest(_) => match t.spine() {
                (head, args) if head.is_lang(Lang::List) && args.len() == 1 => {
                    Some((vec![args[0].clone(); arity], Vec::new(), Vec::new()))
                }
                _ => unknown(),
            },
            Ctor::Bool(_) | Ctor::Unit | Ctor::Literal(_) => unknown(),
        }
    }
}

/// `t` with the rigid variables an arm learned the types of replaced.
pub(super) fn replace_skolems(t: &Type, equals: &[(u32, Type)]) -> Type {
    if equals.is_empty() {
        return t.clone();
    }
    match t {
        Type::Skolem(s) => equals
            .iter()
            .find(|(x, _)| x == s)
            .map_or_else(|| t.clone(), |(_, ty)| replace_skolems(ty, equals)),
        t => t.map_children(&mut |c| replace_skolems(c, equals)),
    }
}
