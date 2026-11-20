//! Non-public type dependencies: ownership is explicit, never a prefix guess.
use std::collections::{BTreeMap, BTreeSet};
use std::mem;

mod variance;

use super::{Entity, Interface, Namespace};
use crate::types::{
    Form, Kind, ModuleNominalKind, ModuleRequirement, ModuleSignature, Type, TypeDef, impl_def,
};
use variance::{validate_definition_variances, validate_signature_variances};

/// A path component is a compiler's atomic static member name. Generated `$`
/// names are valid; separators and empty components cannot denote one edge.
fn atomic(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        && !name.ends_with('$')
        && !name.contains("$$")
}

/// A real class declaration owns a distinct nominal dictionary head. There
/// is intentionally no standalone `TypeDef` for that generated identity.
pub(crate) fn dictionary_head(entity: &Entity, definition: &TypeDef) -> Option<Entity> {
    if entity.namespace != Namespace::Type || !matches!(definition.form, Form::Class { .. }) {
        return None;
    }
    let mut dictionary = entity.clone();
    dictionary.name.push_str("$Dict");
    Some(dictionary)
}

/// Real nominal definitions with explicit canonical identities, including
/// nested signatures. An abstract member without an identity owns no global role.
pub(crate) fn identified_nominals(definition: &TypeDef) -> Vec<(&Entity, &TypeDef)> {
    let mut pending = vec![Part::Definition(definition)];
    let mut found = Vec::new();
    while let Some(part) = pending.pop() {
        match part {
            Part::Definition(definition) => match &definition.form {
                Form::ModuleContract { requirements } => {
                    pending.extend(requirements.iter().map(Part::Requirement));
                }
                Form::ModuleSignature { signature } => pending.push(Part::Signature(signature)),
                _ => {}
            },
            Part::Signature(signature) => match signature {
                ModuleSignature::Structure { requirements } => {
                    pending.extend(requirements.iter().map(Part::Requirement));
                }
                ModuleSignature::Parameterized {
                    parameter, result, ..
                } => pending.extend([Part::Signature(parameter), Part::Signature(result)]),
            },
            Part::Requirement(ModuleRequirement::Module { signature, .. }) => {
                pending.push(Part::Signature(signature));
            }
            Part::Requirement(ModuleRequirement::Nominal {
                identity: Some(entity),
                definition: Some(definition),
                category,
                ..
            }) if entity.namespace == Namespace::Type
                && matches!(
                    (category, &definition.form),
                    (ModuleNominalKind::Class, Form::Class { .. })
                        | (ModuleNominalKind::Service, Form::Service { .. })
                ) =>
            {
                found.push((entity, definition.as_ref()));
            }
            _ => {}
        }
    }
    found
}

pub(crate) fn validate(interface: &Interface) -> Result<(), String> {
    // A private descendant can be represented in its enclosing interface and
    // in a surviving child view. Check their shared identity before learning
    // anything, independently of traversal order. Public/public projections
    // alone are not competing private support authority.
    let mut nested = Vec::new();
    let mut dictionaries = Vec::new();
    let mut generated = Vec::new();
    let mut definitions = BTreeMap::<&Entity, Vec<&TypeDef>>::new();
    let mut records = Vec::new();
    let mut pending = vec![interface];
    while let Some(current) = pending.pop() {
        pending.extend(current.modules.values());
        for export in &current.exports {
            if let Some(definition) = &export.definition {
                validate_definition_variances(
                    &export.entity.module,
                    &export.entity.name,
                    definition,
                )?;
                nested.extend(
                    identified_nominals(definition)
                        .into_iter()
                        .map(|(entity, definition)| (entity, definition, false)),
                );
                dictionaries.extend(dictionary_head(&export.entity, definition));
                definitions
                    .entry(&export.entity)
                    .or_default()
                    .push(definition);
                if let Some(implementation) = impl_def(definition) {
                    let mut entity = export.entity.clone();
                    entity.name.push_str(".Impl");
                    generated.push((entity, implementation, false));
                }
            }
        }
        if let Some(signature) = &current.parameterized {
            validate_signature_variances(&current.module, signature)?;
        }
        for record in &current.support_types {
            validate_definition_variances(
                &record.entity.module,
                &record.entity.name,
                &record.definition,
            )?;
            nested.extend(
                identified_nominals(&record.definition)
                    .into_iter()
                    .map(|(entity, definition)| (entity, definition, true)),
            );
            dictionaries.extend(dictionary_head(&record.entity, &record.definition));
            definitions
                .entry(&record.entity)
                .or_default()
                .push(&record.definition);
            if let Some(implementation) = impl_def(&record.definition) {
                let mut entity = record.entity.clone();
                entity.name.push_str(".Impl");
                generated.push((entity, implementation, true));
            }
            records.push(record);
        }
    }
    for (entity, definition, private) in &nested {
        dictionaries.extend(dictionary_head(entity, definition));
        if let Some(implementation) = impl_def(definition) {
            let mut entity = (*entity).clone();
            entity.name.push_str(".Impl");
            generated.push((entity, implementation, *private));
        }
    }
    for (entity, definition, _) in &generated {
        definitions.entry(entity).or_default().push(definition);
    }
    for dictionary in dictionaries {
        if definitions.contains_key(&dictionary) {
            return Err(format!(
                "nominal class dictionary {}.{} cannot have a type definition",
                dictionary.module, dictionary.name
            ));
        }
    }
    let support_authorities = records
        .iter()
        .map(|record| (&record.entity, &record.definition))
        .chain(
            generated
                .iter()
                .filter(|(_, _, private)| *private)
                .map(|(entity, definition, _)| (entity, definition)),
        );
    for (entity, definition) in support_authorities {
        if definitions[entity].iter().any(|other| *other != definition) {
            return Err(format!(
                "conflicting type support for {}.{}",
                entity.module, entity.name
            ));
        }
    }
    validate_owners(interface)
}

fn validate_owners(interface: &Interface) -> Result<(), String> {
    let public: BTreeSet<_> = interface
        .exports
        .iter()
        .filter(|e| e.definition.is_some())
        .map(|e| &e.entity)
        .collect();
    let mut previous = None;
    for (index, support) in interface.support_types.iter().enumerate() {
        let entity = &support.entity;
        let path = format!("{}.support_types[{index}]", interface.module);
        if entity.namespace != Namespace::Type {
            return Err(format!("{path}: support must have type namespace"));
        }
        if previous.is_some_and(|old| old >= entity) {
            return Err(format!(
                "{path}: support entities must be unique and sorted"
            ));
        }
        previous = Some(entity);
        if public.contains(entity) {
            return Err(format!(
                "{path}: public type definition is duplicated in support"
            ));
        }
        let mut owner = interface.module.clone();
        for member in &support.owner_path {
            if !atomic(member) {
                return Err(format!("{path}: invalid ownership component"));
            }
            owner.push('.');
            owner.push_str(member);
        }
        if entity.module != owner {
            return Err(format!(
                "{path}: entity does not belong to its declared owner path"
            ));
        }
    }
    for child in interface.modules.values() {
        validate_owners(child)?;
    }
    Ok(())
}

enum Part<'a> {
    Type(&'a Type),
    Kind(&'a Kind),
    Definition(&'a TypeDef),
    Signature(&'a ModuleSignature),
    Requirement(&'a ModuleRequirement),
}

fn entity(module: &str, name: &str) -> Entity {
    Entity {
        module: module.to_owned(),
        namespace: Namespace::Type,
        name: name.to_owned(),
    }
}

fn collect(parts: Vec<Part<'_>>) -> BTreeSet<Entity> {
    let mut pending = parts;
    let mut names = BTreeSet::new();
    while let Some(part) = pending.pop() {
        match part {
            Part::Type(ty) => type_dependencies(ty, &mut pending, &mut names),
            Part::Kind(kind) => match kind {
                Kind::Named { module, name } => {
                    names.insert(entity(module, name));
                }
                Kind::Arrow(a, b) => pending.extend([Part::Kind(a), Part::Kind(b)]),
                Kind::Row(inner) => pending.push(Part::Kind(inner)),
                _ => {}
            },
            Part::Definition(def) => {
                pending.push(Part::Kind(&def.kind));
                match &def.form {
                    Form::Alias {
                        body,
                        opaque: false,
                        ..
                    } => pending.push(Part::Type(body)),
                    Form::ModuleContract { requirements } => {
                        pending.extend(requirements.iter().map(Part::Requirement));
                    }
                    Form::ModuleSignature { signature } => pending.push(Part::Signature(signature)),
                    Form::Service { operations, .. } => {
                        pending.extend(operations.iter().map(|o| Part::Type(&o.ty)));
                    }
                    Form::Class {
                        supers,
                        default_sigs,
                        ..
                    } => {
                        pending.extend(supers.iter().map(Part::Type));
                        pending.extend(default_sigs.iter().map(|(_, ty)| Part::Type(ty)));
                    }
                    _ => {}
                }
            }
            Part::Signature(signature) => match signature {
                ModuleSignature::Structure { requirements } => {
                    pending.extend(requirements.iter().map(Part::Requirement));
                }
                ModuleSignature::Parameterized {
                    parameter, result, ..
                } => pending.extend([Part::Signature(parameter), Part::Signature(result)]),
            },
            Part::Requirement(requirement) => match requirement {
                ModuleRequirement::Type {
                    params, manifest, ..
                } => {
                    pending.extend(params.iter().map(|p| Part::Kind(&p.kind)));
                    pending.extend(manifest.iter().map(Part::Type));
                }
                ModuleRequirement::Value { ty, .. }
                | ModuleRequirement::Dictionary { head: ty, .. } => pending.push(Part::Type(ty)),
                ModuleRequirement::Module { signature, .. } => {
                    pending.push(Part::Signature(signature));
                }
                ModuleRequirement::Kind {
                    identity,
                    definition,
                    ..
                } => {
                    names.extend(identity.iter().cloned());
                    pending.push(Part::Definition(definition));
                }
                ModuleRequirement::Nominal {
                    identity,
                    definition,
                    methods,
                    ..
                } => {
                    names.extend(identity.iter().cloned());
                    pending.extend(definition.iter().map(|d| Part::Definition(d)));
                    pending.extend(methods.iter().map(|(_, ty)| Part::Type(ty)));
                }
            },
        }
    }
    names
}

fn type_dependencies<'a>(ty: &'a Type, pending: &mut Vec<Part<'a>>, names: &mut BTreeSet<Entity>) {
    if let Type::Exists {
        scope: _,
        binder,
        contract,
        body,
    } = ty
    {
        // The binder scopes the body, not its contract. This also
        // keeps closed Named kinds under their real contract rather
        // than discovering them as arbitrary global type owners.
        pending.push(Part::Type(contract));
        let scope = format!("$exists:{binder}");
        names.extend(
            collect(vec![Part::Type(body)])
                .into_iter()
                .filter(|entity| {
                    entity.module != scope
                        && !entity
                            .module
                            .strip_prefix(&scope)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                }),
        );
        return;
    }
    if let Type::Con { module, name } = ty {
        // A lexical member's declaration is carried by Exists.contract.
        if !module.starts_with("$exists:") && !module.starts_with("$module:") {
            names.insert(entity(module, name));
        }
    }
    type_children(ty, pending);
}

fn type_children<'a>(ty: &'a Type, pending: &mut Vec<Part<'a>>) {
    match ty {
        Type::App { head, arg } => pending.extend([Part::Type(head), Part::Type(arg)]),
        Type::Fun {
            param,
            effects,
            result,
        } => {
            pending.extend([Part::Type(param), Part::Type(result)]);
            if let Some(effects) = effects {
                pending.extend([Part::Type(&effects.services), Part::Type(&effects.errors)]);
            }
        }
        Type::Tuple(items) => pending.extend(items.iter().map(Part::Type)),
        Type::Record(row) | Type::Row(row) => {
            pending.extend(row.entries.iter().map(|(_, ty)| Part::Type(ty)));
            pending.extend(row.tail.iter().map(|ty| Part::Type(ty)));
        }
        Type::Union(a, b)
        | Type::Captured {
            ty: a, captures: b, ..
        }
        | Type::Bounded { ty: a, captures: b }
        | Type::Exists {
            contract: a,
            body: b,
            ..
        } => pending.extend([Part::Type(a), Part::Type(b)]),
        Type::Without { row, .. } => pending.push(Part::Type(row)),
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            pending.extend(vars.iter().map(|v| Part::Kind(&v.kind)));
            pending.extend(constraints.iter().map(Part::Type));
            pending.push(Part::Type(body));
        }
        _ => {}
    }
}

pub(crate) fn definition_dependencies(definition: &TypeDef) -> BTreeSet<Entity> {
    collect(vec![Part::Definition(definition)])
}

pub(crate) fn dependencies(interface: &Interface) -> BTreeSet<Entity> {
    let mut parts = Vec::new();
    for export in &interface.exports {
        parts.extend(export.scheme.iter().map(Part::Type));
        parts.extend(export.definition.iter().map(Part::Definition));
    }
    parts.extend(
        interface
            .support_types
            .iter()
            .map(|s| Part::Definition(&s.definition)),
    );
    parts.extend(interface.parameterized.iter().map(Part::Signature));
    for instance in &interface.instances {
        parts.push(Part::Type(&instance.head));
        parts.extend(instance.context.iter().map(Part::Type));
        parts.extend(instance.vars.iter().map(|v| Part::Kind(&v.kind)));
    }
    let mut names = collect(parts);
    for child in interface.modules.values() {
        names.extend(dependencies(child));
    }
    names
}

/// Keep only the finite closure reachable from the projected public view.
pub(crate) fn prune(interface: &mut Interface) {
    let records = mem::take(&mut interface.support_types);
    let mut pending = dependencies(interface);
    let mut seen = BTreeSet::new();
    while let Some(entity) = pending.pop_first() {
        if !seen.insert(entity.clone()) {
            continue;
        }
        if let Some(record) = records.iter().find(|record| record.entity == entity) {
            pending.extend(definition_dependencies(&record.definition));
        }
    }
    let public: BTreeSet<_> = interface.exports.iter().map(|e| &e.entity).collect();
    interface.support_types = records
        .into_iter()
        .filter(|r| seen.contains(&r.entity) && !public.contains(&r.entity))
        .collect();
}
