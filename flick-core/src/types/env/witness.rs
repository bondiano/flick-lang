//! Private witness footprints. No source names, values or instances are learned.

use std::collections::{BTreeMap, HashSet};

use super::{Env, Form, Interface, Namespace, TypeDef};
use crate::iface::{Entity, support};
use crate::types::module_contract::WitnessRefresh;
use crate::types::services::impl_def;
use crate::types::ty::carrier;

struct Footprint {
    definitions: BTreeMap<Entity, TypeDef>,
    aliases: HashSet<Entity>,
}

fn within(module: &str, root: &str) -> bool {
    module == root
        || module
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn footprint(view: &Interface) -> Result<Footprint, String> {
    let mut out = Footprint {
        definitions: BTreeMap::new(),
        aliases: HashSet::new(),
    };
    let mut pending = vec![(view, view.module.clone())];
    while let Some((current, path)) = pending.pop() {
        if current.module != path {
            return Err("witness child is not an exact owned member".into());
        }
        for (name, child) in &current.modules {
            pending.push((child, format!("{path}.{name}")));
        }
        for export in &current.exports {
            if export.entity.namespace != Namespace::Type || export.entity.module != path {
                continue;
            }
            let Some(definition) = &export.definition else {
                continue;
            };
            insert_definition(
                &mut out.definitions,
                export.entity.clone(),
                definition.clone(),
            )?;
            if matches!(definition.form, Form::Alias { opaque: false, .. }) {
                out.aliases.insert(export.entity.clone());
            }
            if let Some(implementation) = impl_def(definition) {
                let mut entity = export.entity.clone();
                entity.name.push_str(".Impl");
                insert_definition(&mut out.definitions, entity, implementation)?;
            }
        }
    }
    Ok(out)
}

fn insert_definition(
    definitions: &mut BTreeMap<Entity, TypeDef>,
    entity: Entity,
    definition: TypeDef,
) -> Result<(), String> {
    if let Some(previous) = definitions.get(&entity)
        && (previous != &definition
            || carrier::definition_stamps(previous) != carrier::definition_stamps(&definition))
    {
        return Err("conflicting owned witness definitions".into());
    }
    definitions.insert(entity, definition);
    Ok(())
}

fn retain_owned(view: &mut Interface) {
    view.exports.retain(|export| {
        export.entity.namespace == Namespace::Type && export.entity.module == view.module
    });
    view.support_types.clear();
    view.instances.clear();
    for child in view.modules.values_mut() {
        retain_owned(child);
    }
}

impl Env<'_> {
    /// Collision detection only; a path spelling never grants witness authority.
    pub(in crate::types) fn has_witness_namespace(&self, root: &str) -> bool {
        let definition = self
            .types
            .0
            .iter()
            .chain(&self.builtins.types.0)
            .any(|(module, definitions)| !definitions.is_empty() && within(module, root));
        let value = self
            .values
            .keys()
            .chain(self.builtins.values.keys())
            .any(|entity| within(&entity.module, root));
        let marker = self
            .witness_manifest_aliases
            .iter()
            .chain(&self.support_authorities)
            .any(|entity| within(&entity.module, root));
        let mut pending: Vec<_> = self
            .interfaces
            .values()
            .filter_map(Option::as_ref)
            .chain(self.builtins.interfaces.values().map(AsRef::as_ref))
            .collect();
        let mut interface = false;
        while let Some(current) = pending.pop() {
            interface |= within(&current.module, root);
            pending.extend(current.modules.values());
        }
        definition
            || value
            || marker
            || interface
            || self
                .known_nominals()
                .iter()
                .any(|(entity, _)| within(&entity.module, root))
    }

    pub(in crate::types) fn validate_witness_footprint(
        &self,
        view: &Interface,
    ) -> Result<(), String> {
        let expected = footprint(view)?;
        if self
            .support_authorities
            .iter()
            .chain(self.support_roles.keys())
            .any(|entity| within(&entity.module, &view.module))
        {
            return Err("witness namespace acquired foreign support authority".into());
        }
        for (entity, definition) in &expected.definitions {
            let Some(current) = self.types.get(&entity.module, &entity.name) else {
                return Err("issued witness definition is missing".into());
            };
            if current != definition
                || carrier::definition_stamps(current) != carrier::definition_stamps(definition)
            {
                return Err("issued witness footprint was replaced".into());
            }
            if self.support_authorities.contains(entity) || self.support_roles.contains_key(entity)
            {
                return Err("witness footprint acquired foreign support authority".into());
            }
        }
        for (module, definitions) in &self.types.0 {
            if !within(module, &view.module) {
                continue;
            }
            for name in definitions.keys() {
                let entity = Entity {
                    module: module.clone(),
                    namespace: Namespace::Type,
                    name: name.clone(),
                };
                if !expected.definitions.contains_key(&entity) {
                    return Err("unissued type in witness namespace".into());
                }
            }
        }
        let aliases: HashSet<_> = self
            .witness_manifest_aliases
            .iter()
            .filter(|entity| within(&entity.module, &view.module))
            .cloned()
            .collect();
        if aliases != expected.aliases {
            return Err("issued witness alias provenance was replaced".into());
        }
        Ok(())
    }
    pub(in crate::types) fn refresh_witness_footprint(
        &mut self,
        refresh: &WitnessRefresh<'_>,
    ) -> Result<(), String> {
        let (old, new) = refresh.views();
        if old.module != new.module {
            return Err("witness refresh changes its namespace".into());
        }
        self.validate_witness_footprint(old)?;
        let before = footprint(old)?;
        let after = footprint(new)?;
        for entity in after.definitions.keys() {
            if !before.definitions.contains_key(entity)
                && self.type_def(&entity.module, &entity.name).is_some()
            {
                return Err("witness refresh would overwrite an unissued definition".into());
            }
        }
        let mut owned = new.clone();
        retain_owned(&mut owned);
        support::validate(&owned)?;
        self.check_known_companions_excluding(
            &owned,
            &before.definitions.keys().cloned().collect(),
        )?;
        // No fallible validation after this point. Update only the exact issued
        // slots and their proven replacements, never a foreign/fixed identity.
        for entity in before.definitions.keys() {
            self.types.remove(&entity.module, &entity.name);
        }
        for (entity, definition) in after.definitions {
            self.types.define(&entity.module, entity.name, definition);
        }
        self.witness_manifest_aliases
            .retain(|entity| !before.aliases.contains(entity));
        self.witness_manifest_aliases.extend(after.aliases);
        Ok(())
    }
}
