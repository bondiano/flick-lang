//! Whole-value bounds on generic data and constructor fields.

use super::{Hold, Visible};
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::types::Checker;
use crate::types::env::Form;
use crate::types::kinds::Kind;
use crate::types::ty::{Norm, Type};
use crate::types::unify::Unified;

impl Checker<'_> {
    pub fn bound_head(&self, t: &Type) -> Type {
        match self.qualified_head(t) {
            Type::Bounded { ty, .. } | Type::Captured { ty, .. } if scalar(&self.whnf(&ty)) => {
                self.qualified_head(&ty)
            }
            Type::Captured {
                ty,
                captures,
                value: true,
            } => {
                if self.hidden_fields(&ty, &mut Vec::new()) {
                    let value = matches!(
                        self.whnf(&ty).spine().0,
                        Type::Var(_) | Type::Meta(_) | Type::Skolem(_)
                    );
                    Type::Captured {
                        ty,
                        captures,
                        value,
                    }
                } else {
                    *ty
                }
            }
            other => other,
        }
    }

    /// At a value boundary, parameter-only data has no hidden storage
    /// identity. Keep qualifiers inside type arguments invariant.
    pub fn value_bound(&self, t: Type) -> Type {
        match self.qualified_head(&t) {
            Type::Bounded { ty, .. } | Type::Captured { ty, .. } if scalar(&self.whnf(&ty)) => {
                self.qualified_head(&ty)
            }
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.value_bound(*ty)),
                captures,
            },
            Type::Captured { ty, captures, .. } => {
                if self.hidden_fields(&ty, &mut Vec::new()) {
                    Type::Captured {
                        ty,
                        captures,
                        value: true,
                    }
                } else {
                    Type::Bounded { ty, captures }
                }
            }
            _ => t,
        }
    }

    pub fn capture_identity(&self, t: &Type) -> Type {
        let mut t = self.bound_head(t);
        while let Type::Bounded { ty, .. } = t {
            t = self.bound_head(&ty);
        }
        t
    }

    fn hidden_fields(&self, t: &Type, seen: &mut Vec<(String, String)>) -> bool {
        let t = self.qualified_head(t);
        match &t {
            Type::Captured { ty, .. } | Type::Bounded { ty, .. } => self.hidden_fields(ty, seen),
            Type::Fun { .. } => true,
            Type::Tuple(items) => items.iter().any(|t| self.hidden_fields(t, seen)),
            Type::Record(row) => row.entries.iter().any(|(_, t)| self.hidden_fields(t, seen)),
            Type::Forall { body, .. } => self.hidden_fields(body, seen),
            Type::Con { .. } | Type::App { .. } => {
                let (head, args) = t.spine();
                let Type::Con { module, name } = head else {
                    return true;
                };
                if head.is_prim("List") {
                    return args.iter().any(|t| self.hidden_fields(t, seen));
                }
                if ["Number", "BigInt", "String", "Bool", "Unit", "Never"]
                    .iter()
                    .any(|n| head.is_prim(n))
                {
                    return false;
                }
                let Some(Form::Adt { constructors, .. }) =
                    self.env.type_def(module, name).map(|d| &d.form)
                else {
                    return true;
                };
                if seen.iter().any(|(m, n)| m == module && n == name) {
                    return false;
                }
                seen.push((module.clone(), name.clone()));
                constructors.iter().any(|name| {
                    let entity = Entity {
                        module: module.clone(),
                        namespace: Namespace::Constructor,
                        name: name.clone(),
                    };
                    let Some(scheme) = self.env.value(&entity) else {
                        return true;
                    };
                    let instantiated;
                    let mut current = match scheme {
                        Type::Forall { vars, body, .. } if !args.is_empty() => {
                            let subst: Vec<_> = vars
                                .iter()
                                .zip(&args)
                                .map(|(var, arg)| (var.name.clone(), (**arg).clone()))
                                .collect();
                            instantiated = body.substitute(&subst);
                            &instantiated
                        }
                        _ => scheme,
                    };
                    while let Type::Forall { body, .. } = current {
                        current = body;
                    }
                    while let Type::Fun { param, result, .. } = current {
                        if self.hidden_fields(param, seen) {
                            return true;
                        }
                        current = result;
                    }
                    false
                })
            }
            _ => false,
        }
    }

    pub fn data_value(&self, ty: &Type, captures: Norm) -> Type {
        Type::Bounded {
            ty: Box::new(ty.map_children(&mut |t| self.capture_identity(t))),
            captures: Box::new(captures.into_type()),
        }
    }

    fn refinable_result(&self, result: &Type) -> bool {
        let (bare, captures) = self.whnf_captured(result);
        if let Some(captures) = captures {
            return self
                .store
                .normalize(&captures)
                .entries
                .iter()
                .any(|(label, ty)| {
                    label.starts_with('^')
                        && !matches!(
                            self.atom_kind(ty, &[]),
                            Some(Kind::Region | Kind::CaptureRow)
                        )
                });
        }
        matches!(bare, Type::Fun { .. })
            || bare.contains(&mut |ty| match ty {
                Type::Var(_) | Type::Skolem(_) => !self.is_region(ty, &[]),
                Type::Meta(_) => true,
                _ => false,
            })
    }

    /// Comparing later stages must keep the callable's proven bound.
    pub(super) fn bounded_callable(&self, ty: Type, captures: &Type) -> Type {
        let Type::Fun {
            param,
            effects,
            result,
        } = ty
        else {
            return ty;
        };
        if !self.refinable_result(&result) {
            return Type::Fun {
                param,
                effects,
                result,
            };
        }
        let argument = self.captures_of(&param).into_type();
        let captures = self
            .store
            .normalize(&Type::Union(Box::new(captures.clone()), Box::new(argument)))
            .into_type();
        Type::Fun {
            param,
            effects,
            result: Box::new(Type::Bounded {
                ty: result,
                captures: Box::new(captures),
            }),
        }
    }

    /// A bound refines a complete generic type; unlike a nominal qualifier,
    /// it cannot replace that type's own hidden captures on instantiation.
    pub fn subsume_bound(&mut self, actual: &Type, expected: &Type, span: Span) -> Option<Unified> {
        let mark = self.regions.holds.len();
        let fits = self.subsume_bound_at(actual, expected, span)?;
        Some(self.held_fits(fits, mark, actual, expected, span))
    }

    fn subsume_bound_at(&mut self, actual: &Type, expected: &Type, span: Span) -> Option<Unified> {
        let a = self.bound_head(actual);
        let e = self.bound_head(expected);
        if matches!(e, Type::Bounded { .. }) {
            let mut base = e;
            while let Type::Bounded { ty, captures } = base {
                self.hold(Hold {
                    held: vec![(None, actual.clone())],
                    visible: Visible::Now(Vec::new()),
                    bound: *captures.clone(),
                    expected: Some(expected.clone()),
                    written: !matches!(self.store.resolve(&captures), Type::Meta(_)),
                    what: "value",
                    span,
                });
                base = self.bound_head(&ty);
            }
            let identity = self.capture_identity(&a);
            return Some(self.unify(&identity, &base));
        }
        if let Type::Bounded { ty, captures } = &a
            && !matches!(e, Type::Captured { .. })
        {
            let identity = self.capture_identity(ty);
            if self.hidden_fields(expected, &mut Vec::new()) {
                self.hold(Hold {
                    held: vec![(None, actual.clone())],
                    visible: Visible::Refined(expected.clone()),
                    bound: Type::empty_row(),
                    expected: Some(expected.clone()),
                    written: true,
                    what: "value",
                    span,
                });
            }
            let callable = self.bounded_callable(identity, captures);
            return Some(self.subsume(&callable, expected, span));
        }
        None
    }

    /// Pure parametric code can return only captures from its function and
    /// argument. Refine abstract results, not independent concrete scalars.
    pub fn application_result(&self, result: Type, function: &Type, argument: &Type) -> Type {
        let result = self.value_bound(result);
        if !self.refinable_result(&result)
            || matches!(
                self.whnf(function),
                Type::Fun {
                    effects: Some(_),
                    ..
                }
            )
        {
            return result;
        }
        let mut captures = self.captures_of(function);
        let arg = self.captures_of(argument);
        captures.entries.extend(arg.entries);
        captures.tails.extend(arg.tails);
        // A reader region is the lending function's own, not data a result
        // stores; its holds report every escape.
        captures
            .entries
            .retain(|(_, atom)| !matches!(atom, Type::Skolem(s) if self.regions.lent.contains(s)));
        Type::Bounded {
            ty: Box::new(result),
            captures: Box::new(captures.into_type()),
        }
    }

    /// A projection refines an existing field; it never creates a new
    /// hidden closure environment in that field's complete type.
    pub fn data_field(&self, ty: &Type, captures: &Type) -> Type {
        if scalar(&self.whnf(ty))
            || matches!(
                self.bound_head(ty),
                Type::Bounded { .. } | Type::Captured { .. }
            )
        {
            ty.clone()
        } else {
            Type::Bounded {
                ty: Box::new(ty.clone()),
                captures: Box::new(captures.clone()),
            }
        }
    }

    /// Constructor payloads and the constructed value share a capture
    /// bound. Delayed data results also retain their supplied environment.
    pub fn constructor_field(&self, ty: &Type, captures: &Type) -> Type {
        let head = self.whnf(ty);
        if ["Number", "BigInt", "String", "Bool", "Unit", "Never"]
            .iter()
            .any(|name| head.is_prim(name))
        {
            return ty.clone();
        }
        if matches!(ty, Type::Captured { .. } | Type::Bounded { .. }) {
            return ty.clone();
        }
        if let Type::Forall {
            vars,
            constraints,
            body,
        } = ty
        {
            return Type::Forall {
                vars: vars.clone(),
                constraints: constraints.clone(),
                body: Box::new(self.constructor_field(body, captures)),
            };
        }
        let complete = matches!(head.spine().0, Type::Con { module, name }
            if matches!(self.env.type_def(module, name).map(|d| &d.form),
                Some(Form::Nominal)));
        let ty = match ty {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param_captures = self.captures_of(param).into_type();
                let bound = Type::Union(Box::new(captures.clone()), Box::new(param_captures));
                Type::Fun {
                    param: param.clone(),
                    effects: effects.clone(),
                    result: Box::new(self.constructor_field(result, &bound)),
                }
            }
            other => other.clone(),
        };
        if complete || matches!(ty, Type::Var(_) | Type::Skolem(_) | Type::Meta(_)) {
            Type::Bounded {
                ty: Box::new(ty),
                captures: Box::new(captures.clone()),
            }
        } else {
            Type::Captured {
                ty: Box::new(ty),
                captures: Box::new(captures.clone()),
                value: false,
            }
        }
    }
}

pub(super) fn scalar(t: &Type) -> bool {
    ["Number", "BigInt", "String", "Bool", "Unit", "Never"]
        .iter()
        .any(|name| t.is_prim(name))
}
