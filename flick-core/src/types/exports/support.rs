use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use super::{Checker, Decl, Entity, Form, Interface, ModuleSignature, TypeDef};
use crate::iface::{SupportType, support};

impl Checker<'_> {
    pub(in crate::types) fn collect_support_types(
        &self,
        interface: &Interface,
    ) -> Result<Vec<SupportType>, String> {
        // `extend`, not `collect`: collecting compiles a sort of `Entity` into the wasm compiler.
        let mut public = BTreeSet::new();
        public.extend(
            interface
                .exports
                .iter()
                .filter(|e| e.definition.is_some())
                .map(|e| e.entity.clone()),
        );
        let mut roots = interface.clone();
        roots.support_types.clear();
        let mut queue = support::dependencies(&roots);
        let mut seen = BTreeSet::new();
        let mut found = BTreeMap::new();
        while let Some(mut entity) = queue.pop_first() {
            if !seen.insert(entity.clone())
                || public.contains(&entity)
                || self.env.is_builtin(&entity.module)
            {
                continue;
            }
            // Dictionary and implementation companions depend on their real
            // class/service declaration, not a fabricated missing definition.
            let companion = entity
                .name
                .strip_suffix("$Dict")
                .map(|base| (base, true))
                .or_else(|| entity.name.strip_suffix(".Impl").map(|base| (base, false)));
            if let Some((base, dictionary)) = companion
                && self
                    .env
                    .type_def(&entity.module, base)
                    .is_some_and(|definition| {
                        matches!(
                            (&definition.form, dictionary),
                            (Form::Class { .. }, true) | (Form::Service { .. }, false)
                        )
                    })
            {
                let base = base.to_owned();
                entity.name.clone_from(&base);
            }
            if public.contains(&entity) {
                continue;
            }
            let supported = if entity.module == interface.module && entity.module == self.env.module
            {
                let definition =
                    self.env
                        .type_def(&entity.module, &entity.name)
                        .ok_or_else(|| {
                            format!(
                                "missing owned type support for {}.{}",
                                entity.module, entity.name
                            )
                        })?;
                Some(SupportType {
                    entity: entity.clone(),
                    definition: self.support_definition(&entity, definition),
                    owner_path: Vec::new(),
                })
            } else {
                self.owned_support(interface, &entity)
            };
            if let Some(record) = supported {
                queue.extend(support::definition_dependencies(&record.definition));
                found.insert(entity, record);
            }
        }
        Ok(found.into_values().collect())
    }

    fn support_definition(&self, entity: &Entity, definition: &TypeDef) -> TypeDef {
        if matches!(definition.form, Form::Service { .. }) {
            // Match public and refreshed nominal service snapshots: operation
            // schemes use the import-safe exporter, including transparent alias
            // normalization, while opaque operation results keep their identity.
            return self.exported_def(
                (&entity.module, &entity.name),
                definition,
                Some(Decl::Service),
            );
        }
        if matches!(
            definition.form,
            Form::Alias { opaque: true, .. } | Form::Adt { .. }
        ) {
            return self.exported_def(
                (&entity.module, &entity.name),
                definition,
                Some(Decl::Opaque),
            );
        }
        let mut definition = definition.clone();
        match &mut definition.form {
            Form::ModuleContract { requirements } => {
                let mut signature = ModuleSignature::Structure {
                    requirements: mem::take(requirements),
                };
                signature.map_types(&mut |ty| self.zonk(ty));
                let ModuleSignature::Structure {
                    requirements: solved,
                } = signature
                else {
                    unreachable!()
                };
                *requirements = solved;
            }
            Form::ModuleSignature { signature } => {
                signature.map_types(&mut |ty| self.zonk(ty));
            }
            _ => {}
        }
        // Resolve late contract solutions, but retain alias formals and named
        // references. In particular, do not use exported() for private alias
        // bodies: that would unfold their instantiated named references.
        definition
    }

    fn owned_support(&self, interface: &Interface, entity: &Entity) -> Option<SupportType> {
        for known in self.env.known_interfaces() {
            let Some(prefix) = self.support_owner_path(&interface.module, known) else {
                continue;
            };
            if let Some(record) = known.support_types.iter().find(|s| s.entity == *entity) {
                let mut record = record.clone();
                let mut path = prefix;
                path.extend(record.owner_path);
                record.owner_path = path;
                return Some(record);
            }
            if let Some(export) = known.exports.iter().find(|e| e.entity == *entity)
                && let Some(definition) = &export.definition
                && entity.module == known.module
            {
                return Some(SupportType {
                    entity: entity.clone(),
                    definition: definition.clone(),
                    owner_path: prefix,
                });
            }
        }
        None
    }

    fn support_owner_path(&self, root: &str, interface: &Interface) -> Option<Vec<String>> {
        let mut current = interface;
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        while current.module != root {
            if !seen.insert(current.module.as_str()) {
                return None;
            }
            let member = current.member_of.as_ref()?;
            if current.module != format!("{}.{}", member.parent, member.name) {
                return None;
            }
            path.splice(0..0, member.name.split('.').map(str::to_owned));
            if member.parent == root {
                break;
            }
            current = self.env.interface(&member.parent)?;
        }
        Some(path)
    }
}
