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

fn exported_methods(interface: &Interface, class: &Entity) -> Vec<(String, Type)> {
    interface
        .exports
        .iter()
        .filter(|method| {
            method.parent.as_deref() == Some(&class.name)
                && method.entity.module == class.module
                && method.decl == Decl::Method
        })
        .filter_map(|method| Some((method.entity.name.clone(), method.scheme.clone()?)))
        .collect()
}

fn exported_class_method(interface: &Interface, method: &Export) -> bool {
    method.decl == Decl::Method
        && interface.exports.iter().any(|class| {
            class.decl == Decl::Class
                && class.entity.module == method.entity.module
                && method.parent.as_deref() == Some(&class.entity.name)
        })
}

impl ModuleSignature {
    pub(crate) fn normalize_in(&mut self, witness: &Interface) {
        self.map_types(&mut |ty| super::module_contract::expand_actual(ty, witness, 32));
    }

    /// A broken body must not leak inference variables to another checker,
    /// but its declared inputs still constrain applications and cache keys.
    pub(crate) fn inputs_only(&self) -> Self {
        match self {
            Self::Parameterized {
                group_size,
                binder,
                parameter,
                result,
            } => Self::Parameterized {
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
                Self::Parameterized { .. } => Form::ModuleSignature {
                    signature: Box::new(self.clone()),
                },
            },
        }
    }
    pub fn from_interface(interface: &Interface) -> Self {
        if let Some(signature) = &interface.parameterized {
            return signature.clone();
        }

        let mut requirements = Vec::new();
        for export in &interface.exports {
            match export.entity.namespace {
                Namespace::Value => {
                    if exported_class_method(interface, export) {
                        continue;
                    }
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
                                    methods: exported_methods(interface, &export.entity),
                                });
                            }
                            Form::ModuleContract { .. } | Form::ModuleSignature { .. } => {}
                            Form::Kind | Form::KindMember => requirements.push(R::Kind {
                                name: export.entity.name.clone(),
                                identity: (export.entity.module != interface.module)
                                    .then(|| export.entity.clone()),
                                definition: Box::new(definition.clone()),
                            }),
                            form => {
                                let params = inferred_type_parameters(definition);
                                let manifest = if let Form::Alias { body, .. } = form {
                                    Some(body.clone())
                                } else if export.entity.module != interface.module {
                                    Some(Type::apps(
                                        Type::con(&export.entity.module, &export.entity.name),
                                        params.iter().map(|p| Type::Var(p.name.clone())),
                                    ))
                                } else {
                                    None
                                };
                                requirements.push(R::Type {
                                    name: export.entity.name.clone(),
                                    params,
                                    manifest,
                                });
                            }
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
                        .parameterized
                        .clone()
                        .unwrap_or_else(|| Self::from_interface(child)),
                ),
            });
        }
        let names: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                R::Type { name, .. }
                | R::Kind {
                    name,
                    identity: None,
                    ..
                } => Some((name.clone(), false)),
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
}

#[cfg(test)]
#[path = "module_signature/tests.rs"]
mod tests;

include!("module_signature/display.rs");

include!("module_signature/projection.rs");

include!("module_signature/substitution.rs");
