//! The types of the columns of the exhaustiveness check: the fields of a
//! constructor at the type of the value it is matched against, and whether
//! that value can be built by it at all.

use super::refine::Refined;
use super::scheme::{built, is_hidden_captures};
use crate::iface::{Entity, Namespace};
use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::patterns::Ctor;
use crate::types::ty::Type;

/// What each rigid variable of a matched type equals, as a constructor tells.
type Equals = Vec<(u32, Type)>;

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
        self.ctor_fields_at(&ctor, ty, 0).is_some()
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
    ) -> Option<Vec<Type>> {
        let (mut columns, equals) = self.ctor_fields_at(ctor, ty, arity)?;
        columns.extend(rest.iter().map(|t| replace_skolems(t, &equals)));
        Some(columns)
    }

    /// The types of the `arity` fields of `ctor` in a value of type `ty`, and
    /// what it tells about the rigid variables of `ty`; `None` when no value
    /// of `ty` is built by it. A type that is not known makes its fields not
    /// known either (`Error`).
    fn ctor_fields_at(&self, ctor: &Ctor, ty: &Type, arity: usize) -> Option<(Vec<Type>, Equals)> {
        let unknown = || Some((vec![Type::Error; arity], Vec::new()));
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
                let locals: Vec<String> = vars
                    .iter()
                    .filter(|v| !is_hidden_captures(v))
                    .map(|v| v.name.clone())
                    .collect();
                let (bound, equals) = match self.refine(&locals, built(body), &t) {
                    Refined::Impossible => return None,
                    Refined::Unifier { bound, equals } => (bound, equals),
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
                    Some((fields, equals))
                } else {
                    unknown()
                }
            }
            Ctor::Tuple(_) => match t {
                Type::Tuple(items) if items.len() == arity => Some((items, Vec::new())),
                _ => unknown(),
            },
            Ctor::List(_) | Ctor::ListRest(_) => match t.spine() {
                (head, args) if head.is_lang(Lang::List) && args.len() == 1 => {
                    Some((vec![args[0].clone(); arity], Vec::new()))
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
