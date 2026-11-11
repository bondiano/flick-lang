//! Static module signatures and their dependency-preserving operations.

use std::mem;

use super::{
    Form, Kind, ModuleNominalKind, ModuleRequirement as R, ModuleSignature, Type, TypeDef,
    show_type,
};
use crate::api_diff::canonical_type;
use crate::iface::{Decl, Entity, Export, Interface, Namespace};

fn display_type(ty: &Type) -> String {
    fn scoped(ty: &Type) -> Type {
        if let Type::Con { module, name } = ty
            && let Some(alias) = module.strip_prefix("$display:")
        {
            return Type::con("", &format!("{alias}.{name}"));
        }
        ty.map_children(&mut scoped)
    }
    show_type(&scoped(ty))
}

/// Module substitution changes service/error identities and their derived
/// set keys together. Record field labels are source names and stay fixed.
fn map_module_children(ty: &Type, f: &mut dyn FnMut(&Type) -> Type) -> Type {
    let mut mapped = ty.map_children(f);
    if let (Type::Row(before), Type::Row(after)) = (ty, &mut mapped) {
        for ((old_key, old), (key, new)) in before.entries.iter().zip(&mut after.entries) {
            if let (
                Type::Con { module, name },
                Type::Con {
                    module: actual,
                    name: member,
                },
            ) = (old.spine().0, new.spine().0)
                && *old_key == format!("{module}.{name}")
            {
                *key = format!("{actual}.{member}");
            }
        }
        after.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
    mapped
}

fn map_definition_types(definition: &mut TypeDef, f: &mut dyn FnMut(&Type) -> Type) {
    match &mut definition.form {
        Form::Alias { body, .. } => *body = f(body),
        Form::Class {
            supers,
            default_sigs,
            ..
        } => {
            for ty in supers
                .iter_mut()
                .chain(default_sigs.iter_mut().map(|(_, ty)| ty))
            {
                *ty = f(ty);
            }
        }
        Form::Service { operations, .. } => {
            for operation in operations {
                operation.ty = f(&operation.ty);
            }
        }
        Form::ModuleContract { requirements } => {
            let mut signature = ModuleSignature::Structure {
                requirements: mem::take(requirements),
            };
            signature.map_types(f);
            let ModuleSignature::Structure {
                requirements: mapped,
            } = signature
            else {
                unreachable!()
            };
            *requirements = mapped;
        }
        Form::ModuleSignature { signature } => signature.map_types(f),
        _ => {}
    }
}

fn map_interface_types(interface: &mut Interface, f: &mut dyn FnMut(&Type) -> Type) {
    for export in &mut interface.exports {
        if let Some(ty) = &mut export.scheme {
            *ty = f(ty);
        }
        if let Some(definition) = &mut export.definition {
            map_definition_types(definition, f);
        }
    }
    if let Some(signature) = &mut interface.functor {
        signature.map_types(f);
    }
    for child in interface.modules.values_mut() {
        map_interface_types(child, f);
    }
}

fn map_requirement_types(requirement: &mut R, f: &mut dyn FnMut(&Type) -> Type) {
    match requirement {
        R::Type {
            manifest: Some(ty), ..
        }
        | R::Value { ty, .. } => *ty = f(ty),
        R::Dictionary { head, .. } => *head = f(head),
        R::Module { signature, .. } => signature.map_types(f),
        R::Nominal {
            identity,
            definition,
            methods,
            ..
        } => {
            if let Some(entity) = identity
                && let Type::Con { module, name } = f(&Type::con(&entity.module, &entity.name))
            {
                entity.module = module;
                entity.name = name;
            }
            for (_, ty) in methods {
                *ty = f(ty);
            }
            if let Some(definition) = definition {
                map_definition_types(definition, f);
            }
        }
        R::Type { manifest: None, .. } => {}
    }
}

/// Equality checking may use transparent definitions in a known argument;
/// its abstraction-preserving parameter projection stays unchanged.
pub(crate) fn normalize_module_interface(interface: &mut Interface, witness: &Interface) {
    map_interface_types(interface, &mut |ty| {
        super::module_contract::expand_actual(ty, witness, 32)
    });
}

fn inferred_type_parameters(definition: &TypeDef) -> Vec<super::ty::Binder> {
    let names = match &definition.form {
        Form::Alias { params, .. } => params.as_slice(),
        _ => &[],
    };
    let mut parameters = Vec::new();
    let mut kind = &definition.kind;
    while let Kind::Arrow(parameter, result) = kind {
        let index = parameters.len();
        parameters.push(super::ty::Binder {
            name: names
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("a{index}")),
            kind: (**parameter).clone(),
        });
        kind = result;
    }
    parameters
}

/// Local declarations bind fresh result members. Services and errors use
/// different labels while these identities are represented by bound variables.
fn generalize_local_type(ty: &Type, module: &str, names: &[(String, bool)]) -> Type {
    let binding = |name: &str| {
        names
            .iter()
            .find(|(known, _)| known == name || name.strip_suffix("$Dict") == Some(known))
    };
    if let Type::Con {
        module: owner,
        name,
    } = ty
        && owner == module
        && binding(name).is_some()
    {
        return Type::Var(name.clone());
    }
    let mut mapped = map_module_children(ty, &mut |ty| generalize_local_type(ty, module, names));
    if let (Type::Row(before), Type::Row(after)) = (ty, &mut mapped) {
        for ((old_key, old), (key, new)) in before.entries.iter().zip(&mut after.entries) {
            if let (
                Type::Con {
                    module: owner,
                    name,
                },
                Type::Var(variable),
            ) = (old.spine().0, new.spine().0)
                && owner == module
                && *old_key == format!("{owner}.{name}")
                && let Some((_, service)) = binding(variable)
            {
                *key = if *service {
                    super::ty::service_label(new)
                } else {
                    super::ty::error_label(new)
                }
                .expect("a bound row atom");
            }
        }
        after.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
    mapped
}

impl ModuleSignature {
    pub(crate) fn normalize_in(&mut self, witness: &Interface) {
        self.map_types(&mut |ty| super::module_contract::expand_actual(ty, witness, 32));
    }

    /// A broken body must not leak inference variables to another checker,
    /// but its declared inputs still constrain applications and cache keys.
    pub(crate) fn inputs_only(&self) -> Self {
        match self {
            Self::Functor {
                group_size,
                binder,
                parameter,
                result,
            } => Self::Functor {
                group_size: *group_size,
                binder: binder.clone(),
                parameter: parameter.clone(),
                result: Box::new(result.inputs_only()),
            },
            Self::Structure { .. } => Self::Structure {
                requirements: Vec::new(),
            },
        }
    }
    pub(crate) fn definition(&self) -> TypeDef {
        TypeDef {
            kind: Kind::Module,
            form: match self {
                Self::Structure { requirements } => Form::ModuleContract {
                    requirements: requirements.clone(),
                },
                Self::Functor { .. } => Form::ModuleSignature {
                    signature: Box::new(self.clone()),
                },
            },
        }
    }
    pub fn from_interface(interface: &Interface) -> Self {
        if let Some(signature) = &interface.functor {
            return signature.clone();
        }

        let mut requirements = Vec::new();
        for export in &interface.exports {
            match export.entity.namespace {
                Namespace::Value => {
                    if let Some(ty) = &export.scheme {
                        requirements.push(R::Value {
                            name: export.entity.name.clone(),
                            ty: ty.clone(),
                        });
                    }
                }
                Namespace::Type => {
                    if let Some(definition) = &export.definition {
                        match &definition.form {
                            Form::Class { .. } | Form::Service { .. } => {
                                requirements.push(R::Nominal {
                                    name: export.entity.name.clone(),
                                    category: if matches!(definition.form, Form::Class { .. }) {
                                        ModuleNominalKind::Class
                                    } else {
                                        ModuleNominalKind::Service
                                    },
                                    identity: (export.entity.module != interface.module)
                                        .then(|| export.entity.clone()),
                                    definition: Some(Box::new(definition.clone())),
                                    methods: Vec::new(),
                                });
                            }
                            Form::ModuleContract { .. } | Form::ModuleSignature { .. } => {}
                            form => requirements.push(R::Type {
                                name: export.entity.name.clone(),
                                params: inferred_type_parameters(definition),
                                manifest: if let Form::Alias { body, .. } = form {
                                    Some(body.clone())
                                } else {
                                    None
                                },
                            }),
                        }
                    }
                }
                Namespace::Constructor => {}
            }
        }
        for (name, child) in &interface.modules {
            requirements.push(R::Module {
                name: name.clone(),
                binder: String::new(),
                signature: Box::new(
                    child
                        .functor
                        .clone()
                        .unwrap_or_else(|| Self::from_interface(child)),
                ),
            });
        }
        let names: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                R::Type { name, .. } => Some((name.clone(), false)),
                R::Nominal {
                    name,
                    identity: None,
                    category,
                    ..
                } => Some((name.clone(), *category == ModuleNominalKind::Service)),
                _ => None,
            })
            .collect();
        let mut signature = Self::Structure { requirements };

        signature.map_types(&mut |ty| generalize_local_type(ty, &interface.module, &names));
        signature
    }
    /// Substitute an argument's exported identities, rather than inventing
    /// identities below the name of a module alias.
    pub(crate) fn instantiate_parameter(&mut self, binder: &str, actual: &Interface) {
        fn member(ty: &Type, marker: &str, actual: &Interface) -> Type {
            if let Type::Con { module, name } = ty {
                let suffix = if module == marker {
                    Some("")
                } else {
                    module
                        .strip_prefix(marker)
                        .and_then(|s| s.strip_prefix('.'))
                };
                if let Some(suffix) = suffix {
                    let child = suffix
                        .split('.')
                        .filter(|s| !s.is_empty())
                        .try_fold(actual, |child, name| child.modules.get(name));
                    if let Some(child) = child {
                        if let Some(export) = child.find(Namespace::Type, name) {
                            return Type::con(&export.entity.module, &export.entity.name);
                        }
                        if let Some(class) = name.strip_suffix("$Dict")
                            && let Some(export) = child.find(Namespace::Type, class)
                        {
                            return Type::con(
                                &export.entity.module,
                                &format!("{}$Dict", export.entity.name),
                            );
                        }
                        return Type::con(&child.module, name);
                    }
                }
            }
            map_module_children(ty, &mut |ty| member(ty, marker, actual))
        }
        let marker = format!("$argument({binder})({})", actual.module);
        self.rename_parameter(binder, &marker);
        self.map_types(&mut |ty| member(ty, &marker, actual));
    }

    /// Resolve structure-bound paths through the supplied child identities.
    pub(crate) fn instantiate_members(requirements: &[R], actual: &Interface) -> Vec<R> {
        let mut signature = Self::Structure {
            requirements: requirements.to_vec(),
        };
        for requirement in requirements {
            if let R::Module { name, binder, .. } = requirement
                && !binder.is_empty()
                && let Some(child) = actual.modules.get(name)
            {
                signature.instantiate_parameter(binder, child);
            }
        }
        let Self::Structure { requirements } = signature else {
            unreachable!()
        };
        requirements
    }

    pub fn has_functor(&self) -> bool {
        match self {
            Self::Functor { .. } => true,
            Self::Structure { requirements } => requirements
                .iter()
                .any(|r| matches!(r, R::Module { signature, .. } if signature.has_functor())),
        }
    }
}

#[cfg(test)]
mod tests;

include!("module_signature/transform.rs");

include!("module_signature/interface.rs");

include!("module_signature/display.rs");

include!("module_signature/contract.rs");
