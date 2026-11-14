use std::collections::{BTreeMap, BTreeSet};
use std::iter::once;

use super::{Env, Interface, Namespace, TypeDef};
use crate::iface::{Entity, support};
use crate::types::Form;
use crate::types::services::impl_def;

/// Companion authority retained across public namespace projections. These
/// are validation markers, never global type definitions or source bindings.
#[derive(Debug)]
pub(super) enum CompanionRole {
    Class,
    Service(TypeDef),
}

impl Env<'_> {
    /// Learn semantic definitions without exposing any source name or value.
    /// The conflict check is complete before any support definition is installed.
    pub(crate) fn learn_type_support(&mut self, interface: &Interface) -> Result<(), String> {
        support::validate(interface)?;
        self.check_known_companions(interface)?;
        let mut pending = vec![interface];
        let mut definitions = BTreeMap::<Entity, TypeDef>::new();
        while let Some(current) = pending.pop() {
            pending.extend(current.modules.values());
            let public = current.exports.iter().filter_map(|e| {
                e.definition
                    .as_ref()
                    .map(|definition| (&e.entity, definition))
            });
            let private = current
                .support_types
                .iter()
                .map(|s| (&s.entity, &s.definition));
            // Public projections may deliberately abstract an already known
            // definition. They are not competing support authority.
            for (entity, definition) in public {
                if entity.namespace == Namespace::Type
                    && self.type_def(&entity.module, &entity.name).is_none()
                {
                    definitions
                        .entry(entity.clone())
                        .or_insert_with(|| definition.clone());
                    if let Some(implementation) = impl_def(definition) {
                        let mut generated = entity.clone();
                        generated.name.push_str(".Impl");
                        if self.type_def(&generated.module, &generated.name).is_none() {
                            definitions.entry(generated).or_insert(implementation);
                        }
                    }
                }
            }
            for (entity, definition) in private {
                if entity.namespace != Namespace::Type {
                    continue;
                }
                if let Some(old) = definitions.insert(entity.clone(), definition.clone())
                    && old != *definition
                {
                    return Err(format!(
                        "conflicting type support for {}.{}",
                        entity.module, entity.name
                    ));
                }
                if let Some(implementation) = impl_def(definition) {
                    let mut entity = entity.clone();
                    entity.name.push_str(".Impl");
                    if let Some(old) = definitions.insert(entity.clone(), implementation.clone())
                        && old != implementation
                    {
                        return Err(format!(
                            "conflicting generated type support for {}.{}",
                            entity.module, entity.name
                        ));
                    }
                }
            }
        }
        for (entity, definition) in &definitions {
            if let Some(old) = self.type_def(&entity.module, &entity.name)
                && old != definition
            {
                return Err(format!(
                    "type support cannot overwrite {}.{}",
                    entity.module, entity.name
                ));
            }
        }
        for (entity, definition) in definitions {
            self.types.define(&entity.module, entity.name, definition);
        }
        self.record_support_authorities(interface);
        Ok(())
    }
    /// Register only the named types constructed in a validated witness view.
    /// Foreign identity-fixed declarations and all source/value bindings stay put.
    pub(in crate::types) fn register_witness_types(
        &mut self,
        witness: &str,
        view: &Interface,
    ) -> Result<(), String> {
        fn owned(view: &mut Interface, path: &str) -> Result<(), String> {
            if view.module != path {
                return Err(format!(
                    "witness view {} is not owned by {path}",
                    view.module
                ));
            }
            view.exports.retain(|export| {
                export.entity.namespace == Namespace::Type && export.entity.module == path
            });
            view.support_types.clear();
            view.instances.clear();
            for (name, child) in &mut view.modules {
                owned(child, &format!("{path}.{name}"))?;
            }
            Ok(())
        }
        let mut view = view.clone();
        owned(&mut view, witness)?;
        let mut pending = vec![&view];
        let mut aliases = Vec::new();
        while let Some(current) = pending.pop() {
            pending.extend(current.modules.values());
            for export in &current.exports {
                let Some(definition) = &export.definition else {
                    continue;
                };
                if self
                    .type_def(&export.entity.module, &export.entity.name)
                    .is_some_and(|known| known != definition)
                {
                    return Err(format!(
                        "conflicting witness type {}.{}",
                        export.entity.module, export.entity.name
                    ));
                }
                if let Some(implementation) = impl_def(definition)
                    && self
                        .type_def(
                            &export.entity.module,
                            &format!("{}.Impl", export.entity.name),
                        )
                        .is_some_and(|known| *known != implementation)
                {
                    return Err(format!(
                        "conflicting witness service {}.{}",
                        export.entity.module, export.entity.name
                    ));
                }
                if matches!(definition.form, Form::Alias { opaque: false, .. }) {
                    aliases.push(export.entity.clone());
                }
            }
        }
        self.learn_type_support(&view)?;
        self.witness_manifest_aliases.extend(aliases);
        Ok(())
    }

    pub(in crate::types) fn is_witness_manifest_alias(&self, module: &str, name: &str) -> bool {
        self.witness_manifest_aliases.contains(&Entity {
            module: module.into(),
            namespace: Namespace::Type,
            name: name.into(),
        })
    }

    pub(super) fn record_support_authorities(&mut self, interface: &Interface) {
        let mut pending = vec![interface];
        while let Some(current) = pending.pop() {
            pending.extend(current.modules.values());
            for record in &current.support_types {
                self.support_authorities.insert(record.entity.clone());
                let declarations = once((&record.entity, &record.definition))
                    .chain(support::identified_nominals(&record.definition));
                for (entity, definition) in declarations {
                    self.support_authorities.insert(entity.clone());
                    if matches!(definition.form, Form::Class { .. }) {
                        self.support_roles
                            .insert(entity.clone(), CompanionRole::Class);
                    }
                    if let Some(derived) = impl_def(definition) {
                        self.support_roles
                            .insert(entity.clone(), CompanionRole::Service(derived));
                        let mut implementation = entity.clone();
                        implementation.name.push_str(".Impl");
                        self.support_authorities.insert(implementation);
                    }
                }
            }
        }
    }

    /// Canonical roles are checked metadata only, not global type definitions.
    pub(super) fn known_nominals(&self) -> Vec<(Entity, &TypeDef)> {
        self.known_nominals_excluding(&BTreeSet::new())
    }

    fn known_nominals_excluding(&self, excluded: &BTreeSet<Entity>) -> Vec<(Entity, &TypeDef)> {
        let mut roles = Vec::new();
        for (module, definitions) in self.types.0.iter().chain(&self.builtins.types.0) {
            for (name, definition) in definitions {
                let entity = Entity {
                    module: module.clone(),
                    namespace: Namespace::Type,
                    name: name.clone(),
                };
                if excluded.contains(&entity) {
                    continue;
                }
                if matches!(definition.form, Form::Class { .. } | Form::Service { .. }) {
                    roles.push((entity, definition));
                }
                roles.extend(
                    support::identified_nominals(definition)
                        .into_iter()
                        .map(|(entity, definition)| (entity.clone(), definition)),
                );
            }
        }
        roles
    }

    fn check_known_companions(&self, interface: &Interface) -> Result<(), String> {
        self.check_known_companions_excluding(interface, &BTreeSet::new())
    }

    pub(super) fn check_known_companions_excluding(
        &self,
        interface: &Interface,
        excluded: &BTreeSet<Entity>,
    ) -> Result<(), String> {
        let known = self.known_nominals_excluding(excluded);
        let mut pending = vec![interface];
        while let Some(current) = pending.pop() {
            pending.extend(current.modules.values());
            let public = current.exports.iter().filter_map(|export| {
                export
                    .definition
                    .as_ref()
                    .map(|definition| (&export.entity, definition, false))
            });
            let private = current
                .support_types
                .iter()
                .map(|record| (&record.entity, &record.definition, true));
            for (entity, definition, private) in public.chain(private) {
                self.check_companion(entity, definition, private, &known, excluded)?;
                for (entity, definition) in support::identified_nominals(definition) {
                    self.check_companion(entity, definition, private, &known, excluded)?;
                }
            }
        }
        Ok(())
    }

    fn check_companion(
        &self,
        entity: &Entity,
        definition: &TypeDef,
        private: bool,
        known: &[(Entity, &TypeDef)],
        excluded: &BTreeSet<Entity>,
    ) -> Result<(), String> {
        if entity.namespace != Namespace::Type {
            return Ok(());
        }
        if let Some(dictionary) = support::dictionary_head(entity, definition)
            && !excluded.contains(&dictionary)
            && self
                .type_def(&dictionary.module, &dictionary.name)
                .is_some()
        {
            return Err(format!(
                "nominal class dictionary {}.{} already has a type definition",
                dictionary.module, dictionary.name
            ));
        }
        self.check_support_roles(entity, definition)?;
        for (owner, declared) in known {
            if support::dictionary_head(owner, declared).as_ref() == Some(entity) {
                return Err(format!(
                    "nominal class dictionary {}.{} cannot have a type definition",
                    entity.module, entity.name
                ));
            }
            let mut implementation = owner.clone();
            implementation.name.push_str(".Impl");
            if &implementation == entity
                && (private
                    || self.support_authorities.contains(owner)
                    || self.support_authorities.contains(entity))
                && impl_def(declared).is_some_and(|derived| derived != *definition)
            {
                return Err(format!(
                    "conflicting service implementation for {}.{}",
                    entity.module, entity.name
                ));
            }
            if owner == entity
                && (private || self.support_authorities.contains(owner))
                && let (Some(before), Some(after)) = (impl_def(declared), impl_def(definition))
                && before != after
            {
                return Err(format!(
                    "conflicting service support for {}.{}",
                    entity.module, entity.name
                ));
            }
        }
        if let Some(implementation) = impl_def(definition) {
            let mut generated = entity.clone();
            generated.name.push_str(".Impl");
            if (private
                || self.support_authorities.contains(entity)
                || self.support_authorities.contains(&generated))
                && !excluded.contains(&generated)
                && self
                    .type_def(&generated.module, &generated.name)
                    .is_some_and(|before| *before != implementation)
            {
                return Err(format!(
                    "conflicting service implementation for {}.{}",
                    entity.module, entity.name
                ));
            }
        }
        Ok(())
    }
    fn check_support_roles(&self, entity: &Entity, definition: &TypeDef) -> Result<(), String> {
        for (owner, role) in &self.support_roles {
            let mut companion = owner.clone();
            match role {
                CompanionRole::Class => {
                    companion.name.push_str("$Dict");
                    if &companion == entity {
                        return Err(format!(
                            "nominal class dictionary {}.{} cannot have a type definition",
                            entity.module, entity.name
                        ));
                    }
                }
                CompanionRole::Service(expected) => {
                    companion.name.push_str(".Impl");
                    if (&companion == entity && definition != expected)
                        || (owner == entity
                            && impl_def(definition).is_some_and(|actual| actual != *expected))
                    {
                        return Err(format!(
                            "conflicting service support for {}.{}",
                            entity.module, entity.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iface::SupportType;
    use crate::resolve;
    use crate::source::SourceMap;
    use crate::syntax;
    use crate::types::{Builtins, Form, Kind};

    include!("support/learning_tests.rs");
    include!("support/authority_tests.rs");
}
