//! Structural matching of a named module against `type module` requirements.

use super::env::{Form, ModuleNominalKind, ModuleRequirement, TypeDef};
use super::ty::Type;
use crate::api_diff;
use crate::diagnostics::codes;
use crate::iface::{Decl, Export, Interface, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug, Clone)]
pub(super) struct Opened {
    pub alias: String,
    pub types: Vec<(String, Type, super::Kind)>,
    pub values: Vec<(String, Type)>,
}

type OpenedRequirements = (
    Vec<(String, Type)>,
    Vec<(String, Type, super::Kind)>,
    Vec<(String, Type)>,
);

fn open_existential_type(ty: &Type, binder: &str, witness: &str) -> Type {
    if let Type::Con { module, name } = ty
        && let Some(suffix) = within_witness(module, &format!("$exists:{binder}"))
    {
        return Type::con(&format!("{witness}{suffix}"), name);
    }
    if matches!(ty, Type::Exists { binder: inner, .. } if inner == binder) {
        return ty.clone();
    }
    ty.map_children(&mut |child| open_existential_type(child, binder, witness))
}

fn close_existential_type(ty: &Type, witness: &str, binder: &str) -> Type {
    if let Type::Con { module, name } = ty
        && let Some(suffix) = within_witness(module, witness)
    {
        return Type::con(&format!("$exists:{binder}{suffix}"), name);
    }
    ty.map_children(&mut |child| close_existential_type(child, witness, binder))
}

include!("module_contract/checker.rs");
include!("module_contract/runtime.rs");

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mismatch {
    Missing(String),
    Kind(String),
    TypeEquality(String),
    ValueType(String),
    NominalIdentity(String),
    Shape(String),
}

include!("module_contract/kinds.rs");

fn contract_subst(
    requirements: &[ModuleRequirement],
    actual: &Interface,
    errors: &mut Vec<Mismatch>,
    kinds: &[(super::Kind, super::Kind)],
) -> Vec<(String, Type)> {
    let mut subst = Vec::new();
    for requirement in requirements {
        match requirement {
            ModuleRequirement::Type { name, params, .. } => {
                let Some(export) = actual.find(Namespace::Type, name) else {
                    errors.push(Mismatch::Missing(name.clone()));
                    continue;
                };
                let Some(def) = &export.definition else {
                    errors.push(Mismatch::Kind(name.clone()));
                    continue;
                };
                let expected =
                    super::Kind::arrows(params.iter().map(|p| p.kind.clone()), super::Kind::Type);
                if def.kind != substitute_kind(&expected, kinds) {
                    errors.push(Mismatch::Kind(name.clone()));
                }
                let representation = match &def.form {
                    Form::Alias { params, body, .. } if params.is_empty() => body.clone(),
                    _ => Type::con(&export.entity.module, &export.entity.name),
                };
                subst.push((name.clone(), representation));
            }
            ModuleRequirement::Kind { name, .. } | ModuleRequirement::Nominal { name, .. } => {
                if let Some(export) = actual.find(Namespace::Type, name) {
                    subst.push((
                        name.clone(),
                        Type::con(&export.entity.module, &export.entity.name),
                    ));
                } else {
                    errors.push(Mismatch::Missing(name.clone()));
                }
            }
            ModuleRequirement::Value { .. }
            | ModuleRequirement::Dictionary { .. }
            | ModuleRequirement::Module { .. } => {}
        }
    }
    subst
}

pub(crate) fn check(contract: &TypeDef, actual: &Interface) -> Vec<Mismatch> {
    if let Form::ModuleSignature { signature } = &contract.form {
        return if signature.accepts(actual) {
            Vec::new()
        } else {
            vec![Mismatch::Shape("parameterized module".into())]
        };
    }
    let Form::ModuleContract { requirements } = &contract.form else {
        return vec![Mismatch::Kind("module contract".into())];
    };
    let mut errors = Vec::new();
    let requirements = super::ModuleSignature::instantiate_members(requirements, actual);
    let kinds = kind_witnesses(&requirements, actual, &mut errors);
    let subst = contract_subst(&requirements, actual, &mut errors, &kinds);
    let normalizer = super::Store::default();
    let same = |expected: &Type, found: &Type| {
        api_diff::canonical_type(&normalizer.zonk(&expand_actual(
            &substitute_type_kinds(&expected.substitute(&subst), &kinds),
            actual,
            32,
        ))) == api_diff::canonical_type(&normalizer.zonk(&expand_actual(found, actual, 32)))
    };
    for requirement in &requirements {
        match requirement {
            ModuleRequirement::Module {
                name, signature, ..
            } => match actual.modules.get(name) {
                None => errors.push(Mismatch::Missing(name.clone())),
                Some(child) => {
                    if !matches_child(signature, child, actual, &subst) {
                        errors.push(Mismatch::Shape(name.clone()));
                    }
                }
            },
            ModuleRequirement::Type {
                name,
                manifest: Some(expected),
                params,
            } => {
                if let Some(export) = actual.find(Namespace::Type, name)
                    && let Some(found) = manifest_body(export, params)
                    && !same(expected, &found)
                {
                    errors.push(Mismatch::TypeEquality(name.clone()));
                }
            }

            ModuleRequirement::Value { name, ty } => match actual.find(Namespace::Value, name) {
                None => errors.push(Mismatch::Missing(name.clone())),
                Some(export) if !export.scheme.as_ref().is_some_and(|found| same(ty, found)) => {
                    errors.push(Mismatch::ValueType(name.clone()));
                }
                Some(_) => {}
            },
            ModuleRequirement::Nominal {
                name,
                category,
                identity,
                definition,
                methods,
            } => {
                let Some(export) = actual.find(Namespace::Type, name) else {
                    continue;
                };
                let right_category = matches!(
                    (category, export.decl),
                    (ModuleNominalKind::Class, Decl::Class)
                        | (ModuleNominalKind::Service, Decl::Service)
                );
                if !right_category {
                    errors.push(Mismatch::Shape(name.clone()));
                } else if let Some(identity) = identity {
                    if &export.entity != identity {
                        errors.push(Mismatch::NominalIdentity(name.clone()));
                    }
                } else if let (Some(expected), Some(found)) = (definition, &export.definition) {
                    if substitute_kind(&expected.kind, &kinds) != found.kind
                        || !shape_matches(expected, found, methods, actual, name, &same)
                    {
                        errors.push(Mismatch::Shape(name.clone()));
                    }
                } else {
                    errors.push(Mismatch::Shape(name.clone()));
                }
            }
            ModuleRequirement::Dictionary { from, .. } => {
                if actual.find(Namespace::Value, from).is_none() {
                    errors.push(Mismatch::Missing(from.clone()));
                }
            }
            ModuleRequirement::Kind { .. } | ModuleRequirement::Type { manifest: None, .. } => {}
        }
    }
    errors
}

/// Compare a parameterized equality under the contract's type binders,
/// independently of the implementation's parameter spelling.
fn manifest_body(export: &Export, params: &[super::ty::Binder]) -> Option<Type> {
    Some(match &export.definition.as_ref()?.form {
        Form::Alias {
            params: found,
            body,
            ..
        } => {
            let names: Vec<_> = found
                .iter()
                .cloned()
                .zip(params.iter().map(|p| Type::Var(p.name.clone())))
                .collect();
            body.substitute(&names)
        }
        _ => Type::apps(
            Type::con(&export.entity.module, &export.entity.name),
            params.iter().map(|p| Type::Var(p.name.clone())),
        ),
    })
}

fn matches_child(
    signature: &super::ModuleSignature,
    child: &Interface,
    parent: &Interface,
    subst: &[(String, Type)],
) -> bool {
    let mut signature = signature.clone();
    signature.substitute_outer_types(subst);
    signature.normalize_in(parent);
    let mut child = child.clone();
    super::normalize_module_interface(&mut child, parent);
    signature.accepts(&child)
}

/// The value type that can supply a constraint's dictionary.
pub(super) fn dictionary_type(head: &Type) -> Option<Type> {
    let (class, args) = head.spine();
    let dictionary = match class {
        Type::Con { module, name } => Type::con(module, &format!("{name}$Dict")),
        Type::Var(name) => Type::Var(format!("{name}$Dict")),
        _ => return None,
    };
    Some(Type::apps(dictionary, args.into_iter().cloned()))
}

/// The part of an argument visible through a parameterized module parameter. Abstract
/// type requirements keep the argument's identity but hide its definition.
/// The exact constraint and exported value supplied by each `instance ...
/// from ...` requirement of a static module argument. These are local to the
/// named result and never become instances in the argument's interface.
pub(crate) fn local_dictionaries(
    contract: &TypeDef,
    actual: &Interface,
) -> Vec<(Type, super::Dict)> {
    let Form::ModuleContract { requirements } = &contract.form else {
        return Vec::new();
    };
    let requirements = super::ModuleSignature::instantiate_members(requirements, actual);
    let subst: Vec<(String, Type)> = requirements
        .iter()
        .filter_map(|requirement| match requirement {
            ModuleRequirement::Type { name, manifest, .. } => {
                let export = actual.find(Namespace::Type, name)?;
                let identity = Type::con(&export.entity.module, &export.entity.name);
                let ty = if manifest.is_some() {
                    expand_actual(&identity, actual, 32)
                } else {
                    identity
                };
                Some((name.clone(), ty))
            }
            ModuleRequirement::Nominal { name, .. } => {
                let export = actual.find(Namespace::Type, name)?;
                Some((
                    name.clone(),
                    Type::con(&export.entity.module, &export.entity.name),
                ))
            }
            _ => None,
        })
        .collect();
    requirements
        .iter()
        .filter_map(|requirement| {
            let ModuleRequirement::Dictionary { head, from } = requirement else {
                return None;
            };
            let export = actual.find(Namespace::Value, from)?;
            Some((
                head.substitute(&subst),
                super::Dict::Value {
                    module: export.entity.module.clone(),
                    name: export.entity.name.clone(),
                },
            ))
        })
        .collect()
}

pub(crate) fn project(contract: &TypeDef, actual: &Interface) -> Interface {
    if let Form::ModuleSignature { signature } = &contract.form {
        return signature.project(actual);
    }
    let Form::ModuleContract { requirements } = &contract.form else {
        return Interface::default();
    };
    let requirements = super::ModuleSignature::instantiate_members(requirements, actual);
    let kinds = kind_witnesses(&requirements, actual, &mut Vec::new());
    let mut projected = Interface {
        module: actual.module.clone(),
        private: actual.private,
        ..Interface::default()
    };
    let subst: Vec<(String, Type)> = requirements
        .iter()
        .filter_map(|requirement| match requirement {
            ModuleRequirement::Type { name, .. }
            | ModuleRequirement::Kind { name, .. }
            | ModuleRequirement::Nominal { name, .. } => {
                let export = actual.find(Namespace::Type, name)?;
                Some((
                    name.clone(),
                    Type::con(&export.entity.module, &export.entity.name),
                ))
            }
            _ => None,
        })
        .collect();
    for requirement in &requirements {
        match requirement {
            ModuleRequirement::Module {
                name, signature, ..
            } => {
                if let Some(actual) = actual.modules.get(name) {
                    let mut signature = (**signature).clone();
                    signature.substitute_outer_types(&subst);
                    projected
                        .modules
                        .insert(name.clone(), signature.project(actual));
                }
            }
            ModuleRequirement::Type {
                name,
                manifest,
                params,
            } => {
                if let Some(export) = actual.find(Namespace::Type, name) {
                    let mut export = export.clone();
                    if let Some(def) = &mut export.definition {
                        def.form = manifest.as_ref().map_or(Form::NOMINAL, |ty| Form::Alias {
                            params: params.iter().map(|p| p.name.clone()).collect(),
                            body: substitute_type_kinds(&ty.substitute(&subst), &kinds),
                            opaque: false,
                        });
                    }
                    projected.exports.push(export);
                }
            }
            ModuleRequirement::Value { name, ty } => {
                if let Some(export) = actual.find(Namespace::Value, name) {
                    let mut export = export.clone();
                    export.scheme = Some(substitute_type_kinds(&ty.substitute(&subst), &kinds));
                    projected.exports.push(export);
                }
            }
            ModuleRequirement::Kind { name, .. } | ModuleRequirement::Nominal { name, .. } => {
                if let Some(export) = actual.find(Namespace::Type, name) {
                    projected.exports.push(export.clone());
                    projected.exports.extend(
                        actual
                            .exports
                            .iter()
                            .filter(|member| {
                                member.parent.as_deref() == Some(name)
                                    && member.entity.namespace == Namespace::Value
                            })
                            .cloned(),
                    );
                }
            }
            ModuleRequirement::Dictionary { .. } => {}
        }
    }
    projected
}

fn shape_matches(
    expected: &TypeDef,
    found: &TypeDef,
    methods: &[(String, Type)],
    actual: &Interface,
    class_name: &str,
    same: &impl Fn(&Type, &Type) -> bool,
) -> bool {
    match (&expected.form, &found.form) {
        (
            Form::Class {
                supers: required_supers,
                fundeps: required_fundeps,
                ..
            },
            Form::Class {
                supers, fundeps, ..
            },
        ) => {
            required_fundeps == fundeps
                && required_supers
                    .iter()
                    .all(|required| supers.iter().any(|provided| same(required, provided)))
                && methods.iter().all(|(name, ty)| {
                    actual.exports.iter().any(|e| {
                        e.parent.as_deref() == Some(class_name)
                            && e.entity.name == *name
                            && e.scheme.as_ref().is_some_and(|found| same(ty, found))
                    })
                })
        }
        (
            Form::Service {
                operations: required,
                ..
            },
            Form::Service { operations, .. },
        ) => required.iter().all(|op| {
            operations
                .iter()
                .any(|provided| op.name == provided.name && same(&op.ty, &provided.ty))
        }),
        _ => false,
    }
}

pub(super) fn expand_actual(ty: &Type, actual: &Interface, fuel: usize) -> Type {
    if fuel == 0 {
        return ty.clone();
    }
    let (head, args) = ty.spine();
    if let Type::Con { module, name } = head
        && let Some(export) = find_type(actual, module, name)
        && export.entity.module == *module
        && let Some(TypeDef {
            form: Form::Alias { params, body, .. },
            ..
        }) = &export.definition
        && params.len() == args.len()
    {
        let replacements: Vec<_> = params
            .iter()
            .cloned()
            .zip(args.into_iter().cloned())
            .collect();
        return expand_actual(&body.substitute(&replacements), actual, fuel - 1);
    }
    ty.map_children(&mut |child| expand_actual(child, actual, fuel - 1))
}

fn find_type<'a>(actual: &'a Interface, module: &str, name: &str) -> Option<&'a Export> {
    actual
        .find(Namespace::Type, name)
        .filter(|e| e.entity.module == module)
        .or_else(|| {
            actual
                .modules
                .values()
                .find_map(|child| find_type(child, module, name))
        })
}

#[cfg(test)]
mod tests;
