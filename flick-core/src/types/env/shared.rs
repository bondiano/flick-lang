//! The built-in modules as every environment sees them.

use std::collections::{HashMap, HashSet};

use super::{Instance, TypeDefs};
use crate::iface::{Entity, Interface, Namespace};
use crate::types::lang;
use crate::types::services::impl_def;
use crate::types::ty::Type;

/// What every module knows of the built-in modules, learned once and shared
/// by the environments of all modules instead of rebuilt for each.
#[derive(Debug, Default)]
pub struct Builtins {
    pub(super) types: TypeDefs,
    pub(super) values: HashMap<Entity, Type>,
    pub(super) parents: HashMap<Entity, String>,
    /// The exports of the prelude, by plain name.
    pub(super) prelude: HashMap<(Namespace, String), Entity>,
    /// Each built-in module, by name; its name is also its qualifier.
    pub(super) interfaces: HashMap<String, Interface>,
    pub(super) instances: Vec<Instance>,
    pub(super) seen: HashSet<(String, String)>,
}

impl Builtins {
    pub fn new(interfaces: &[Interface]) -> Builtins {
        let mut builtins = Builtins::default();
        for interface in interfaces {
            builtins.add(interface);
        }
        builtins
    }

    /// Learns one more built-in module, after the ones it depends on.
    pub fn add(&mut self, interface: &Interface) {
        for instance in &interface.instances {
            if self
                .seen
                .insert((instance.module.clone(), instance.name.clone()))
            {
                self.instances.push(instance.clone());
            }
        }
        learn_exports(
            &mut self.types,
            &mut self.values,
            &mut self.parents,
            interface,
        );
        if interface.module == lang::prelude() {
            for export in &interface.exports {
                self.prelude.insert(
                    (export.entity.namespace, export.entity.name.clone()),
                    export.entity.clone(),
                );
            }
        }
        self.interfaces
            .insert(interface.module.clone(), interface.clone());
    }
}

/// Records the definitions, schemes and constructor parents an interface exports.
pub(super) fn learn_exports(
    types: &mut TypeDefs,
    values: &mut HashMap<Entity, Type>,
    parents: &mut HashMap<Entity, String>,
    interface: &Interface,
) {
    for export in &interface.exports {
        let entity = &export.entity;
        if let Some(def) = &export.definition {
            if let Some(implementation) = impl_def(def) {
                let name = format!("{}.Impl", entity.name);
                types.define(&entity.module, name, implementation);
            }
            types.define(&entity.module, entity.name.clone(), def.clone());
        }
        if let Some(scheme) = &export.scheme {
            values.insert(entity.clone(), scheme.clone());
        }
        if let Some(parent) = &export.parent {
            parents.insert(entity.clone(), parent.clone());
        }
    }
}
