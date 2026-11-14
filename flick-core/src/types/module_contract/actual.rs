//! Kind-indexed traversal of an issued witness's retained actual carrier.

use std::borrow::Cow;

use super::kinded::{self, Bound, Identities, Incomplete, Kinding};
use super::{Form, Interface, Kind, ModuleRequirement as R, Type, TypeDef};
use crate::iface::{Entity, Namespace};
use crate::types::ty::Binder;
use crate::types::{Elem, ModuleSignature};

pub(super) fn map(
    actual: &Interface,
    kinding: &impl Kinding,
    identities: &mut impl Identities,
) -> Result<Interface, Incomplete> {
    Actual {
        kinding,
        identities,
        binders: Vec::new(),
        modules: Vec::new(),
    }
    .interface(actual)
}

struct Actual<'a, K, I> {
    kinding: &'a K,
    identities: &'a mut I,
    binders: Vec<Binder>,
    modules: Vec<(String, ModuleSignature)>,
}

struct LocalKinds<'a, K> {
    inner: &'a K,
    modules: &'a [(String, ModuleSignature)],
}

impl<K: Kinding> Kinding for LocalKinds<'_, K> {
    fn observe<'a>(
        &'a self,
        ty: &'a Type,
        expected: &Kind,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Cow<'a, Type>, Incomplete> {
        self.inner.observe(ty, expected, binders, bound)
    }
    fn observe_label<'a>(
        &'a self,
        label: &'a Elem,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Cow<'a, Elem>, Incomplete> {
        self.inner.observe_label(label, binders, bound)
    }
    fn resolve_kind(&self, kind: &Kind) -> Kind {
        self.inner.resolve_kind(kind)
    }
    fn checked_kind(&self, kind: &Kind) -> Result<Kind, Incomplete> {
        self.inner.checked_kind(kind)
    }
    fn scoped_head_kind(
        &self,
        ty: &Type,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Kind, Incomplete> {
        if let Type::Con { module, name } = ty {
            for (path, signature) in self.modules.iter().rev() {
                if let Some(kind) = signature.header_kind(path, module, name) {
                    return self.inner.checked_kind(&kind);
                }
            }
        }
        self.inner.scoped_head_kind(ty, binders, bound)
    }
    fn head_kind(&self, ty: &Type, binders: &[Binder]) -> Result<Kind, Incomplete> {
        self.scoped_head_kind(ty, binders, &[])
    }
}

impl<K: Kinding, I: Identities> Actual<'_, K, I> {
    fn ty(&mut self, ty: &Type, kind: &Kind) -> Result<Type, Incomplete> {
        kinded::map_in(
            ty,
            kind,
            &LocalKinds {
                inner: self.kinding,
                modules: &self.modules,
            },
            self.identities,
            &self.binders,
            &[],
        )
    }

    fn kind(&mut self, kind: &Kind) -> Result<Kind, Incomplete> {
        Ok(match self.kinding.checked_kind(kind)? {
            Kind::Arrow(a, b) => Kind::arrow(self.kind(&a)?, self.kind(&b)?),
            Kind::Row(inner) => Kind::Row(Box::new(self.kind(&inner)?)),
            kind => self.identities.kind(&kind, &[]),
        })
    }

    fn parameters(&mut self, names: &[String], kind: &Kind) -> Result<Kind, Incomplete> {
        let mut remaining = self.kinding.checked_kind(kind)?;
        for name in names {
            let Kind::Arrow(parameter, result) = remaining else {
                return Err(match remaining {
                    Kind::Meta(_) | Kind::MetaView(_) => Incomplete::Kind(remaining),
                    _ => Incomplete::NotFunction(remaining),
                });
            };
            self.binders.push(Binder {
                name: name.clone(),
                kind: *parameter,
            });
            remaining = self.kinding.checked_kind(&result)?;
        }
        Ok(remaining)
    }

    fn definition(&mut self, definition: &TypeDef) -> Result<TypeDef, Incomplete> {
        let mut result = definition.clone();
        result.kind = self.kind(&definition.kind)?;
        let mark = self.binders.len();
        match &mut result.form {
            Form::Alias { params, body, .. } => {
                let kind = self.parameters(params, &definition.kind)?;
                *body = self.ty(body, &kind)?;
            }
            Form::Service {
                params, operations, ..
            } => {
                self.parameters(params, &definition.kind)?;
                for operation in operations {
                    operation.ty = self.ty(&operation.ty, &Kind::Type)?;
                }
            }
            Form::Class {
                params,
                supers,
                default_sigs,
                ..
            } => {
                self.parameters(params, &definition.kind)?;
                for constraint in supers {
                    *constraint = self.ty(constraint, &Kind::Constraint)?;
                }
                for (_, scheme) in default_sigs {
                    *scheme = self.ty(scheme, &Kind::Type)?;
                }
            }
            Form::ModuleContract { requirements } => self.requirements(requirements)?,
            Form::ModuleSignature { signature } => self.signature(signature)?,
            Form::Nominal { .. }
            | Form::Adt { .. }
            | Form::Error
            | Form::Kind
            | Form::KindMember => {}
        }
        self.binders.truncate(mark);
        Ok(result)
    }

    fn signature(&mut self, signature: &mut ModuleSignature) -> Result<(), Incomplete> {
        match signature {
            ModuleSignature::Structure { requirements } => self.requirements(requirements),
            ModuleSignature::Parameterized {
                binder,
                parameter,
                result,
                ..
            } => {
                let original = (**parameter).clone();
                self.signature(parameter)?;
                self.modules.push((format!("$module:{binder}"), original));
                let mapped = self.signature(result);
                self.modules.pop();
                mapped
            }
        }
    }

    fn identity(&mut self, identity: &mut Option<Entity>) -> Result<(), Incomplete> {
        if let Some(entity) = identity {
            if entity.namespace != Namespace::Type {
                return Err(Incomplete::InvalidOccurrence(
                    super::context::Invalid::Authority,
                ));
            }
            let original = Type::con(&entity.module, &entity.name);
            let Type::Con { module, name } = self.identities.ty(&original, &[]) else {
                return Err(Incomplete::InvalidOccurrence(
                    super::context::Invalid::Contract,
                ));
            };
            entity.module = module;
            entity.name = name;
        }
        Ok(())
    }

    fn requirements(&mut self, requirements: &mut [R]) -> Result<(), Incomplete> {
        let module_mark = self.modules.len();
        // All checked dependent member headers are visible during traversal.
        for requirement in requirements.iter() {
            if let R::Module {
                binder, signature, ..
            } = requirement
                && !binder.is_empty()
            {
                self.modules
                    .push((format!("$module:{binder}"), (**signature).clone()));
            }
        }
        for requirement in requirements {
            match requirement {
                R::Kind {
                    identity,
                    definition,
                    ..
                } => {
                    self.identity(identity)?;
                    **definition = self.definition(definition)?;
                }
                R::Module { signature, .. } => self.signature(signature)?,
                R::Type {
                    params, manifest, ..
                } => {
                    let mark = self.binders.len();
                    self.binders.extend(params.iter().cloned());
                    if let Some(body) = manifest {
                        *body = self.ty(body, &Kind::Type)?;
                    }
                    self.binders.truncate(mark);
                    for param in params {
                        param.kind = self.kind(&param.kind)?;
                    }
                }
                R::Value { ty, .. } => *ty = self.ty(ty, &Kind::Type)?,
                R::Dictionary { head, .. } => *head = self.ty(head, &Kind::Constraint)?,
                R::Nominal {
                    identity,
                    definition,
                    methods,
                    ..
                } => {
                    self.identity(identity)?;
                    if let Some(definition) = definition {
                        **definition = self.definition(definition)?;
                    }
                    for (_, scheme) in methods {
                        *scheme = self.ty(scheme, &Kind::Type)?;
                    }
                }
            }
        }
        self.modules.truncate(module_mark);
        Ok(())
    }

    fn interface(&mut self, actual: &Interface) -> Result<Interface, Incomplete> {
        let mut mapped = actual.clone();
        for export in &mut mapped.exports {
            if let Some(definition) = &mut export.definition {
                *definition = self.definition(definition)?;
            }
            if let Some(scheme) = &mut export.scheme {
                *scheme = self.ty(scheme, &Kind::Type)?;
            }
        }
        for support in &mut mapped.support_types {
            support.definition = self.definition(&support.definition)?;
        }
        if let Some(signature) = &mut mapped.parameterized {
            self.signature(signature)?;
        }
        for instance in &mut mapped.instances {
            let mark = self.binders.len();
            self.binders.extend(instance.vars.iter().cloned());
            instance.head = self.ty(&instance.head, &Kind::Constraint)?;
            for constraint in &mut instance.context {
                *constraint = self.ty(constraint, &Kind::Constraint)?;
            }
            self.binders.truncate(mark);
            for binder in &mut instance.vars {
                binder.kind = self.kind(&binder.kind)?;
            }
        }
        for child in mapped.modules.values_mut() {
            *child = self.interface(child)?;
        }
        Ok(mapped)
    }
}

/// Observe a retained carrier without changing identities. Origin references
/// stay retained separately; this is a fresh proof-time observation only.
pub(super) fn observe(actual: &Interface, kinding: &impl Kinding) -> Result<Interface, Incomplete> {
    map(actual, kinding, &mut Preserve)
}

struct Preserve;
impl Identities for Preserve {
    fn ty(&mut self, ty: &Type, _: &[Bound]) -> Type {
        ty.clone()
    }
    fn kind(&mut self, kind: &Kind, _: &[Bound]) -> Kind {
        kind.clone()
    }
    fn semantic_label(&mut self, label: &Elem, _: &[Bound]) -> Elem {
        label.clone()
    }
    fn finish(&mut self, ty: Type, _: &Kind, _: &[Bound]) -> Type {
        kinded::sort_identities(ty)
    }
}
