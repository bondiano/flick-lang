//! What the checker knows about named entities: type constructors with their
//! kinds and definitions, the types of values and constructors, and which
//! entity a plain or qualified name refers to in the current module.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::kinds::Kind;
use super::ty::{PRELUDE, Type};
use crate::iface::{Entity, Export, Interface, Namespace};
use crate::resolve::{ItemKind, ModuleSyntax};

/// Kind and definition of a named type constructor, as `.iface` stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDef {
    /// For a `kind` declaration, the kind it names.
    pub kind: Kind,
    #[serde(flatten)]
    pub form: Form,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "kebab-case")]
pub enum Form {
    /// Equal only to itself: built-in, abstract, opaque or refined.
    Nominal,
    /// A variant type with every constructor, exported or not.
    Adt {
        constructors: Vec<String>,
    },
    /// `type error E = E T …`: a nominal type with the one constructor `E`.
    Error,
    /// A transparent alias. An opaque alias is one only inside its module.
    Alias {
        params: Vec<String>,
        body: Type,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        opaque: bool,
    },
    Service {
        params: Vec<String>,
        operations: Vec<Operation>,
    },
    Class,
    /// `kind K = …`
    Kind,
    /// A member of a `kind` declaration.
    KindMember,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

/// A name found in scope.
#[derive(Debug, Clone)]
pub enum Found {
    Entity(Entity),
    /// From a module that could not be loaded or checked: anything goes.
    Unknown,
    Missing,
}

#[derive(Debug, Default)]
pub struct Env {
    pub module: String,
    types: HashMap<(String, String), TypeDef>,
    values: HashMap<Entity, Type>,
    /// Type of a constructor's parent, by constructor entity.
    parents: HashMap<Entity, String>,
    /// Local definitions and names imported by name.
    scope: HashMap<(Namespace, String), Entity>,
    /// Names from `exposing (..)` imports.
    open: HashMap<(Namespace, String), Entity>,
    prelude: HashMap<(Namespace, String), Entity>,
    /// Names the checker cannot attribute: two imports bring them as
    /// different entities (name resolution reports that), or they come from
    /// a module that could not be loaded.
    ambiguous: HashSet<(Namespace, String)>,
    /// Import qualifiers and built-in modules, by qualifier.
    qualifiers: HashMap<String, String>,
    interfaces: HashMap<String, Option<Interface>>,
    /// Some open import could not be loaded, so a missing name may exist.
    open_unknown: bool,
}

impl Env {
    /// The environment of `module` before its own declarations are added.
    pub fn new(
        module: &str,
        syntax: &ModuleSyntax,
        imported: &dyn Fn(&str) -> Option<Interface>,
        builtins: &[Interface],
    ) -> Env {
        let mut env = Env {
            module: module.to_owned(),
            ..Env::default()
        };
        for interface in builtins {
            env.qualifiers
                .insert(interface.module.clone(), interface.module.clone());
            env.learn(interface);
            if interface.module == PRELUDE && module != PRELUDE {
                for export in &interface.exports {
                    env.prelude.insert(
                        (export.entity.namespace, export.entity.name.clone()),
                        export.entity.clone(),
                    );
                }
            }
            env.interfaces
                .insert(interface.module.clone(), Some(interface.clone()));
        }
        for import in &syntax.imports {
            let interface = imported(&import.module);
            env.qualifiers
                .insert(import.qualifier().to_owned(), import.module.clone());
            if let Some(i) = &interface {
                env.learn(i);
            }
            env.interfaces.insert(import.module.clone(), interface);
        }
        for import in &syntax.imports {
            let interface = env.interfaces[&import.module].clone();
            let open = import.exposing.iter().any(|i| i.kind == ItemKind::All);
            match interface {
                None => {
                    // names from a module that could not be loaded are taken on trust
                    env.open_unknown |=
                        open || import.exposing.iter().any(|i| i.kind == ItemKind::ConAll);
                    for item in &import.exposing {
                        for namespace in [Namespace::Value, Namespace::Type, Namespace::Constructor]
                        {
                            env.ambiguous.insert((namespace, item.name.clone()));
                        }
                    }
                }
                Some(i) if open => {
                    for export in &i.exports {
                        bring(&mut env.open, &mut env.ambiguous, export);
                    }
                }
                Some(_) => {}
            }
        }
        let mut ambiguous = HashSet::new();
        for import in &syntax.imports {
            let Some(interface) = env.interfaces[&import.module].clone() else {
                continue;
            };
            for item in &import.exposing {
                let named = interface
                    .exports
                    .iter()
                    .filter(|e| e.entity.name == item.name);
                let members = interface
                    .members(&item.name)
                    .filter(|_| item.kind == ItemKind::ConAll);
                for export in named.chain(members).collect::<Vec<_>>() {
                    bring(&mut env.scope, &mut ambiguous, export);
                }
            }
        }
        // a name imported by name hides the same name from `exposing (..)`
        env.ambiguous.retain(|key| !env.scope.contains_key(key));
        env.ambiguous.extend(ambiguous);
        env
    }

    /// Records the types an interface carries.
    fn learn(&mut self, interface: &Interface) {
        for export in &interface.exports {
            let entity = &export.entity;
            if let Some(def) = &export.definition {
                if let Some(implementation) = super::services::impl_def(def) {
                    let name = format!("{}.Impl", entity.name);
                    self.types
                        .insert((entity.module.clone(), name), implementation);
                }
                self.types
                    .insert((entity.module.clone(), entity.name.clone()), def.clone());
            }
            if let Some(scheme) = &export.scheme {
                self.values.insert(entity.clone(), scheme.clone());
            }
            if let Some(parent) = &export.parent {
                self.parents.insert(entity.clone(), parent.clone());
            }
        }
    }

    pub fn entity(&self, namespace: Namespace, name: &str) -> Entity {
        Entity {
            module: self.module.clone(),
            namespace,
            name: name.to_owned(),
        }
    }

    pub fn type_def(&self, module: &str, name: &str) -> Option<&TypeDef> {
        self.types.get(&(module.to_owned(), name.to_owned()))
    }

    pub fn define_type(&mut self, name: &str, def: TypeDef) {
        self.types
            .insert((self.module.clone(), name.to_owned()), def);
    }

    pub fn type_def_mut(&mut self, name: &str) -> Option<&mut TypeDef> {
        self.types.get_mut(&(self.module.clone(), name.to_owned()))
    }

    /// Makes a local definition visible by its plain name.
    pub fn declare(&mut self, namespace: Namespace, name: &str) {
        let entity = self.entity(namespace, name);
        let key = (namespace, name.to_owned());
        self.ambiguous.remove(&key);
        self.scope.insert(key, entity);
    }

    pub fn set_value(&mut self, entity: Entity, scheme: Type) {
        self.values.insert(entity, scheme);
    }

    pub fn set_parent(&mut self, entity: Entity, parent: &str) {
        self.parents.insert(entity, parent.to_owned());
    }

    pub fn value(&self, entity: &Entity) -> Option<&Type> {
        self.values.get(entity)
    }

    /// The type a constructor builds, by its entity.
    pub fn parent(&self, entity: &Entity) -> Option<(String, String)> {
        let parent = self.parents.get(entity)?;
        Some((entity.module.clone(), parent.clone()))
    }

    pub fn find(&self, namespace: Namespace, name: &str) -> Found {
        let key = (namespace, name.to_owned());
        if self.ambiguous.contains(&key) {
            return Found::Unknown;
        }
        match self.scope.get(&key).or_else(|| self.open.get(&key)) {
            Some(entity) => Found::Entity(entity.clone()),
            None if self.open_unknown => Found::Unknown,
            None => match self.prelude.get(&key) {
                Some(entity) => Found::Entity(entity.clone()),
                None => Found::Missing,
            },
        }
    }

    /// The module a qualifier names, and its interface if it was loaded.
    pub fn qualified_module(&self, qualifier: &str) -> Option<(&str, Option<&Interface>)> {
        let module = self.qualifiers.get(qualifier)?;
        let interface = self.interfaces.get(module).and_then(Option::as_ref);
        Some((module, interface))
    }
}

fn bring(
    scope: &mut HashMap<(Namespace, String), Entity>,
    ambiguous: &mut HashSet<(Namespace, String)>,
    export: &Export,
) {
    let key = (export.entity.namespace, export.entity.name.clone());
    if let Some(previous) = scope.insert(key.clone(), export.entity.clone())
        && previous != export.entity
    {
        ambiguous.insert(key);
    }
}
