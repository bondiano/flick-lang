//! What the checker knows about named entities: type constructors with their
//! kinds and definitions, the types of values and constructors, and which
//! entity a plain or qualified name refers to in the current module.

pub use flick_ir::{EnumLit, JsClass, Rep};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use super::kinds::Kind;
use super::services::Operation;
use super::ty::{Binder, Type};
use crate::iface::{Entity, Export, Interface, Namespace};
use crate::resolve::{ItemKind, ModuleSyntax};

mod shared;

use crate::types::lang;
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
    /// Equal only to itself: built-in, abstract, opaque or refined. An
    /// opaque enum keeps its literals, what its values are.
    Nominal {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        literals: Vec<EnumLit>,
        /// An opaque Flick type with hidden storage (`stores_hidden`).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hidden: bool,
    },
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
    /// A structural contract on a module's public members.
    ModuleContract {
        requirements: Vec<ModuleRequirement>,
    },
    ModuleSignature {
        signature: Box<ModuleSignature>,
    },
    Service {
        params: Vec<String>,
        operations: Vec<Operation>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        effect: bool,
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
        /// The scheme a default definition has where the class body repeats
        /// its method's signature with constraints of the default's own.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        default_sigs: Vec<(String, Type)>,
        /// Marked `@derive`: the plugin of a package derives its instances.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        derive: bool,
    },
    /// `kind K = …`
    Kind,
    /// A member of a `kind` declaration.
    KindMember,
}

/// A member required by `type module`; the contract's type members bind
/// names in the types of later requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "member", rename_all = "kebab-case")]
pub enum ModuleRequirement {
    /// A finite kind or a member retained by an inferred module signature.
    Kind {
        name: String,
        identity: Option<Entity>,
        definition: Box<TypeDef>,
    },
    Module {
        name: String,
        /// Lexical identity of this member in dependent sibling requirements.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        binder: String,
        signature: Box<ModuleSignature>,
    },
    Type {
        name: String,
        params: Vec<Binder>,
        manifest: Option<Type>,
    },
    Value {
        name: String,
        ty: Type,
    },
    Nominal {
        name: String,
        category: ModuleNominalKind,
        /// A class or service whose identity the contract fixes.
        identity: Option<Entity>,
        /// The shape of an abstract class or service requirement.
        definition: Option<Box<TypeDef>>,
        /// Method schemes of an abstract class; a service's operations are in its definition.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        methods: Vec<(String, Type)>,
    },
    Dictionary {
        head: Type,
        from: String,
    },
}

/// A checked static module signature. Module arrows are distinct from value
/// arrows: their binders name static identities, not runtime arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "signature", rename_all = "kebab-case")]
pub enum ModuleSignature {
    Structure {
        requirements: Vec<ModuleRequirement>,
    },
    Parameterized {
        /// Number of arguments still required in this atomic parameter group.
        group_size: usize,
        binder: String,
        parameter: Box<ModuleSignature>,
        result: Box<ModuleSignature>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleNominalKind {
    Class,
    Service,
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
    /// `untagged type T = A = "a" | B = "b"`: each constructor is its
    /// literal, given per constructor.
    Literal(Vec<EnumLit>),
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
            Abi::Literal(literals) => literals
                .get(i)
                .map_or(Rep::Tagged, |l| Rep::Literal(l.clone())),
        }
    }
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
    /// Whether a ground mismatch may insert this conversion.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub implicit: bool,
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

/// A name found in scope.
#[derive(Debug, Clone)]
pub enum Found {
    Entity(Entity),
    /// From a module that could not be loaded or checked: anything goes.
    Unknown,
    Missing,
}

/// The definitions of types by module, then by name: a lookup borrows both,
/// and the checker asks for one at every head of a type it reduces.
#[derive(Debug, Default)]
pub(super) struct TypeDefs(HashMap<String, HashMap<String, TypeDef>>);

impl TypeDefs {
    fn get(&self, module: &str, name: &str) -> Option<&TypeDef> {
        self.0.get(module)?.get(name)
    }

    fn remove(&mut self, module: &str, name: &str) {
        if let Some(types) = self.0.get_mut(module) {
            types.remove(name);
        }
    }

    fn define(&mut self, module: &str, name: String, def: TypeDef) {
        match self.0.get_mut(module) {
            Some(types) => types.insert(name, def),
            None => self
                .0
                .entry(module.to_owned())
                .or_default()
                .insert(name, def),
        };
    }

    /// The types `module` defines, in no order.
    fn of(&self, module: &str) -> impl Iterator<Item = (&String, &TypeDef)> {
        self.0.get(module).into_iter().flatten()
    }
}

/// A module's environment: its own and imported knowledge over the shared
/// [`Builtins`], which every lookup falls back to.
#[derive(Debug)]
pub struct Env<'b> {
    pub module: String,
    builtins: &'b Builtins,
    types: TypeDefs,
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
    /// Some open import did not parse in full, so a name found nowhere,
    /// not even in the prelude, may be one its module lost.
    lost_unknown: bool,
    /// The instances this module sees beyond the built-in ones: of the
    /// modules it depends on, however they are imported, and its own.
    instances: Vec<Instance>,
}

pub(super) struct StaticLexical {
    names: Vec<((Namespace, String), Option<Entity>, bool)>,
    instances: usize,
}

include!("env_impl.rs");

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
