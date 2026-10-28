//! What the checker knows about named entities: type constructors with their
//! kinds and definitions, the types of values and constructors, and which
//! entity a plain or qualified name refers to in the current module.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use super::kinds::Kind;
use super::ty::{Binder, PRELUDE, Type};
use crate::iface::{Entity, Export, Interface, Namespace};
use crate::resolve::{ItemKind, ModuleSyntax};

mod shared;

pub use shared::Builtins;
use shared::learn_exports;

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
    /// A variant type with every constructor, exported or not, and how its
    /// values are represented in JS.
    Adt {
        constructors: Vec<String>,
        #[serde(default, skip_serializing_if = "Abi::is_tagged")]
        abi: Abi,
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
    Class {
        params: Vec<String>,
        /// Superclass constraints over the parameters.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        supers: Vec<Type>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fundeps: Vec<Fundep>,
        methods: Vec<String>,
        /// Methods with a default definition in the class body.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        defaults: Vec<String>,
    },
    /// `kind K = …`
    Kind,
    /// A member of a `kind` declaration.
    KindMember,
}

/// The JS representation of a variant type's values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Abi {
    /// `{ _tag, _0, … }`.
    #[default]
    Tagged,
    /// `unboxed type`: the one field of the one constructor.
    Unboxed,
    /// `untagged type`: the one field of each constructor, told apart by
    /// its JS type, given per constructor.
    Untagged(Vec<JsClass>),
}

impl Abi {
    pub fn is_tagged(&self) -> bool {
        *self == Abi::Tagged
    }

    /// The representation of constructor `i`.
    pub fn rep(&self, i: usize) -> Rep {
        match self {
            Abi::Tagged => Rep::Tagged,
            Abi::Unboxed => Rep::Unboxed,
            Abi::Untagged(classes) => classes.get(i).map_or(Rep::Tagged, |c| Rep::Typeof(*c)),
        }
    }
}

/// What JS can tell a value apart by, without a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JsClass {
    Number,
    String,
    Boolean,
    Bigint,
    /// A list or a tuple.
    Array,
}

impl JsClass {
    /// The name the runtime and the generated tests use.
    pub fn name(self) -> &'static str {
        match self {
            JsClass::Number => "number",
            JsClass::String => "string",
            JsClass::Boolean => "boolean",
            JsClass::Bigint => "bigint",
            JsClass::Array => "array",
        }
    }
}

/// How the values of one constructor are represented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rep {
    #[default]
    Tagged,
    Unboxed,
    Typeof(JsClass),
}

/// `a b -> c` of a class: the parameters at `from` determine those at `to`,
/// by position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fundep {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
}

/// An instance of a class. Its head and context are over the variables
/// `vars`; transparent aliases in them are expanded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    /// The module that declares it.
    pub module: String,
    /// The head as written, for diagnostics and dictionary names: `Show (List a)`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<Binder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<Type>,
    /// The class applied to the instance's types.
    pub head: Type,
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

/// A module's environment: its own and imported knowledge over the shared
/// [`Builtins`], which every lookup falls back to.
#[derive(Debug)]
pub struct Env<'b> {
    pub module: String,
    builtins: &'b Builtins,
    types: HashMap<(String, String), TypeDef>,
    values: HashMap<Entity, Type>,
    /// Type of a constructor's parent, by constructor entity.
    parents: HashMap<Entity, String>,
    /// Local definitions and names imported by name.
    scope: HashMap<(Namespace, String), Entity>,
    /// Names from `exposing (..)` imports.
    open: HashMap<(Namespace, String), Entity>,
    /// Whether the prelude's names are in scope: everywhere but the prelude.
    prelude: bool,
    /// Names the checker cannot attribute: two imports bring them as
    /// different entities (name resolution reports that), or they come from
    /// a module that could not be loaded.
    ambiguous: HashSet<(Namespace, String)>,
    /// Import qualifiers, by qualifier; built-in modules are their own.
    qualifiers: HashMap<String, String>,
    /// The qualifiers of the module's imports.
    imported: HashSet<String>,
    interfaces: HashMap<String, Option<Interface>>,
    /// Some open import could not be loaded, so a missing name may exist.
    open_unknown: bool,
    /// The instances this module sees beyond the built-in ones: of the
    /// modules it depends on, however they are imported, and its own.
    instances: Vec<Instance>,
}

impl<'b> Env<'b> {
    /// The environment of `module` before its own declarations are added.
    pub fn new(
        module: &str,
        syntax: &ModuleSyntax,
        imported: &dyn Fn(&str) -> Option<Interface>,
        builtins: &'b Builtins,
    ) -> Env<'b> {
        let mut env = Env {
            module: module.to_owned(),
            builtins,
            types: HashMap::new(),
            values: HashMap::new(),
            parents: HashMap::new(),
            scope: HashMap::new(),
            open: HashMap::new(),
            prelude: module != PRELUDE,
            ambiguous: HashSet::new(),
            qualifiers: HashMap::new(),
            imported: HashSet::new(),
            interfaces: HashMap::new(),
            open_unknown: false,
            instances: Vec::new(),
        };
        for import in &syntax.imports {
            let interface = imported(&import.module);
            env.qualifiers
                .insert(import.qualifier().to_owned(), import.module.clone());
            env.imported.insert(import.qualifier().to_owned());
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

    /// Records the types and instances an interface carries.
    fn learn(&mut self, interface: &Interface) {
        for instance in &interface.instances {
            let known = |i: &Instance| i.module == instance.module && i.name == instance.name;
            if !self
                .builtins
                .seen
                .contains(&(instance.module.clone(), instance.name.clone()))
                && !self.instances.iter().any(known)
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
    }

    /// Every instance this module sees: the built-in ones, then those of the
    /// modules it depends on, however they are imported, and its own.
    pub fn instances(&self) -> impl Iterator<Item = &Instance> {
        self.builtins.instances.iter().chain(&self.instances)
    }

    /// The instances beyond the built-in ones, which an interface carries.
    pub fn own_instances(&self) -> impl Iterator<Item = &Instance> {
        self.instances
            .iter()
            .filter(|i| !self.builtins.contains(&i.module))
    }

    pub fn add_instance(&mut self, instance: Instance) {
        self.instances.push(instance);
    }

    pub fn entity(&self, namespace: Namespace, name: &str) -> Entity {
        Entity {
            module: self.module.clone(),
            namespace,
            name: name.to_owned(),
        }
    }

    pub fn type_def(&self, module: &str, name: &str) -> Option<&TypeDef> {
        let key = (module.to_owned(), name.to_owned());
        self.types
            .get(&key)
            .or_else(|| self.builtins.types.get(&key))
    }

    pub fn define_type(&mut self, name: &str, def: TypeDef) {
        self.types
            .insert((self.module.clone(), name.to_owned()), def);
    }

    #[cfg(feature = "ide")]
    /// The transparent type aliases the module declares, exported or not.
    pub fn local_aliases(&self) -> impl Iterator<Item = (&str, &[String], &Type)> {
        self.types
            .iter()
            .filter_map(|((module, name), def)| match &def.form {
                Form::Alias {
                    params,
                    body,
                    opaque: false,
                } if *module == self.module => Some((name.as_str(), params.as_slice(), body)),
                _ => None,
            })
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
        self.values
            .get(entity)
            .or_else(|| self.builtins.values.get(entity))
    }

    /// The type a constructor builds, by its entity.
    pub fn parent(&self, entity: &Entity) -> Option<(String, String)> {
        let parent = self
            .parents
            .get(entity)
            .or_else(|| self.builtins.parents.get(entity))?;
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
            None => match self.prelude_names().get(&key) {
                Some(entity) => Found::Entity(entity.clone()),
                None => Found::Missing,
            },
        }
    }

    /// The type `path` names: an unqualified one in scope, or `M.T`, `T` as
    /// the module of the import `M` exports it, re-exported or its own.
    pub fn find_type(&self, path: &str) -> Found {
        let Some((qualifier, name)) = path.rsplit_once('.') else {
            return self.find(Namespace::Type, path);
        };
        match self.qualified_module(qualifier) {
            Some((_, Some(interface))) => interface
                .find(Namespace::Type, name)
                .map_or(Found::Missing, |e| Found::Entity(e.entity.clone())),
            Some((_, None)) => Found::Unknown,
            None => Found::Missing,
        }
    }

    /// Every unqualified name in `namespace`: candidates for a misspelt one.
    pub fn names(&self, namespace: Namespace) -> impl Iterator<Item = &str> {
        self.scope
            .keys()
            .chain(self.open.keys())
            .chain(self.prelude_names().keys())
            .filter(move |(n, _)| *n == namespace)
            .map(|(_, name)| name.as_str())
    }

    /// Every import qualifier and built-in module name.
    pub fn qualifier_names(&self) -> impl Iterator<Item = &str> {
        self.qualifiers
            .keys()
            .chain(self.builtins.interfaces.keys())
            .map(String::as_str)
    }

    fn prelude_names(&self) -> &HashMap<(Namespace, String), Entity> {
        static NONE: LazyLock<HashMap<(Namespace, String), Entity>> = LazyLock::new(HashMap::new);
        if self.prelude {
            &self.builtins.prelude
        } else {
            &NONE
        }
    }

    /// The module a qualifier names, and its interface if it was loaded.
    /// Whether `qualifier` names an import, whose missing names name
    /// resolution reports, rather than a built-in module used without one.
    pub fn is_import(&self, qualifier: &str) -> bool {
        self.imported.contains(qualifier)
    }

    /// The representation of constructor `name` of type `ty` of `module`.
    pub fn constructor_rep(&self, module: &str, ty: &str, name: &str) -> Rep {
        match self.type_def(module, ty).map(|d| &d.form) {
            Some(Form::Adt { constructors, abi }) => constructors
                .iter()
                .position(|c| c == name)
                .map_or(Rep::Tagged, |i| abi.rep(i)),
            _ => Rep::Tagged,
        }
    }

    /// The shortest qualifier that names `module`.
    pub fn qualifier_of(&self, module: &str) -> Option<&str> {
        let builtin = self
            .builtins
            .interfaces
            .get_key_value(module)
            .filter(|(q, _)| !self.qualifiers.contains_key(*q))
            .map(|(q, _)| q.as_str());
        self.qualifiers
            .iter()
            .filter(|(_, m)| *m == module)
            .map(|(q, _)| q.as_str())
            .chain(builtin)
            .min_by_key(|q| (q.len(), *q))
    }

    /// The interface of an imported or built-in module.
    pub fn interface(&self, module: &str) -> Option<&Interface> {
        match self.interfaces.get(module) {
            Some(interface) => interface.as_ref(),
            None => self.builtins.interfaces.get(module),
        }
    }

    pub fn qualified_module(&self, qualifier: &str) -> Option<(&str, Option<&Interface>)> {
        let module = match self.qualifiers.get(qualifier) {
            Some(module) => module.as_str(),
            None => self
                .builtins
                .interfaces
                .get_key_value(qualifier)?
                .0
                .as_str(),
        };
        let interface = match self.interfaces.get(module) {
            Some(interface) => interface.as_ref(),
            None => self.builtins.interfaces.get(module),
        };
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
