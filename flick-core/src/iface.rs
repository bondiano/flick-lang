//! `.iface`, the native interface of a module: versioned JSON that an
//! importer reads instead of the module's source. Schema:
//! `docs/src/content/docs/reference/iface.md`: names and their identity, the types and kinds
//! of exports with their captures, classes and instances, and from a build
//! the data and dictionary ABI, summaries of values and fingerprints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::{EnumLit, Instance, ModuleSignature, Type, TypeDef};

mod template;
pub use template::{
    BindingOrigin, CalleeOrigin, ExpansionFrame, LexicalScope, ModuleTemplate, StaticArgument,
    StaticBinding, StaticCapture, StaticMember,
};

/// Bumped on every change of the schema.
pub const IFACE_VERSION: u32 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    Value,
    Type,
    Constructor,
}

/// Identity of a top-level entity: the module that defines it, not the one a
/// name is imported from, so a re-export stays the same entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Entity {
    pub module: String,
    pub namespace: Namespace,
    pub name: String,
}

/// The declaration form of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decl {
    /// `let`, or a signature whose definition is checked later
    Let,
    Foreign,
    /// `foreign blocked name "reason"`: an import that cannot be used
    Blocked,
    /// a method signature of a class
    Method,
    /// `type T = A | B`
    Adt,
    /// transparent `type T = …`
    Alias,
    Opaque,
    /// `type T` without a definition
    Abstract,
    /// `type module Sig = ...`; its body is a structural module contract.
    ModuleType,
    Error,
    Kind,
    /// `A` of `kind K = A | B`
    KindMember,
    Service,
    Class,
    Constructor,
}

impl Decl {
    /// Whether `T(..)` exposes anything: constructors, methods or kind members.
    pub fn has_members(self) -> bool {
        matches!(self, Decl::Adt | Decl::Error | Decl::Kind | Decl::Class)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    #[serde(flatten)]
    pub entity: Entity,
    pub decl: Decl,
    /// Type of a constructor, class of a method, `kind` of a kind member:
    /// `T(..)` exposes the members of `T`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Type of a value or constructor; absent while the module has not been
    /// type-checked (e.g. it has syntax errors).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Type>,
    /// Kind and definition of a type, service, class or kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<TypeDef>,
    /// Why a `foreign blocked` value cannot be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    /// The UTF-8 byte range of the declared name in the source of the
    /// module that declares it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[u32; 2]>,
}

/// A constructor of an exported type that is itself not exported; kept only
/// to explain why importing it fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hidden {
    pub name: String,
    pub parent: String,
}

/// What a module exposes to its importers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interface {
    pub module: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, Interface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameterized: Option<ModuleSignature>,
    /// The parameterized's result was advertised by a declaration or ascription,
    /// rather than inferred from a private implementation body.
    #[serde(default, skip_serializing_if = "is_false")]
    pub signature_explicit: bool,
    /// A static namespace member is imported through its parent's public view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_of: Option<StaticMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(not(feature = "native-project"), serde(skip))]
    pub template: Option<ModuleTemplate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub private: bool,
    /// The module's source did not parse in full, so a declaration may have
    /// lost names: importers take on trust what they do not find here.
    #[serde(default, skip_serializing_if = "is_false")]
    pub incomplete: bool,
    pub exports: Vec<Export>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<Hidden>,
    /// The `///` comment of each export the module declares, by name, so
    /// declarations made from the interface alone (`.d.ts`) carry it too.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub docs: BTreeMap<String, String>,
    /// The instances of the module and of every module it depends on: an
    /// importer sees them all, whatever it imports by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<Instance>,
    /// How compiled code represents what the module defines; written by a build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi: Option<Abi>,
    /// The fingerprint of each interface this one was built against.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    /// The module's source, relative to the root of its package; written by
    /// a build, so editors find a declaration in a published package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The exported values a tag marks, by tag, each with its path to what
    /// the tag is of: names of this module, then an imported one whose
    /// interface goes on. [`MAKER`] marks the functions that make an atom,
    /// an action or a view, which a render must not call; [`SERVER`], the
    /// values that reach `Server`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, Vec<(String, Vec<String>)>>,
    /// The targets a build compiled the module for; empty in an interface
    /// no build wrote.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Target>,
}

// serde's predicate receives a reference to the serialized field.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// A target compiled code of a module exists for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Js,
    Go,
}

/// The ABI of a compiled module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Abi {
    /// The constructors of each type and error the module declares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Data>,
    /// The dictionary of each instance the module declares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dictionaries: Vec<Dictionary>,
    /// Summaries of the module's top-level values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<Summary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Data {
    #[serde(rename = "type")]
    pub ty: String,
    pub constructors: Vec<ConstructorAbi>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructorAbi {
    pub name: String,
    /// The qualified id: package, module, type and name.
    pub id: String,
    /// The `_tag` of its values: the name and a hash of the id; absent for
    /// an `unboxed` or `untagged` type, whose values are their field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Fields `_0` … `_{arity - 1}`.
    pub arity: usize,
    /// The dictionaries a value of a GADT holds after its fields: `$0` … in
    /// JS, the fields after the others in Go.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dicts: usize,
    /// The literal its value is, for a constructor of an enum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<EnumLit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dictionary {
    /// The instance head, as `Dict::Instance` names it.
    pub instance: String,
    /// The JS export of the dictionary.
    pub export: String,
    /// Dictionaries of the context it takes.
    pub context: usize,
    pub methods: Vec<String>,
    /// Superclasses it has a getter `$Class` for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supers: Vec<String>,
}

/// What an optimizing importer may rely on about a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub name: String,
    /// One character per parameter of its definition: `.` pure, `!` runtime.
    pub stages: String,
    /// Dictionaries it takes before its parameters.
    pub dicts: usize,
    /// Whether applying it to all its parameters may throw or not end;
    /// `false` only when that is certain.
    pub partial: bool,
}

/// A fingerprint of the interface: equal fingerprints mean importers
/// compiled against one are valid against the other.
pub fn fingerprint(interface: &Interface) -> String {
    let mut bare = interface.clone();
    bare.dependencies.clear();
    // where declarations are written, and their comments, change nothing for importers
    bare.source = None;
    bare.docs.clear();
    for export in &mut bare.exports {
        export.at = None;
    }
    let json = serde_json::to_string(&bare).expect("an interface serializes");
    let hash = json.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// The tag of the makers of an interface.
pub const MAKER: &str = "maker";
/// The tag of the types whose values hold one value each.
pub const CELL: &str = "cell";
/// The tag of the types whose top-level values are labelled, each with
/// its kind.
pub const LABEL: &str = "label";
/// The tag of the services whose handler owns what is made under them.
pub const OWNER: &str = "owner";
/// The tag of what reaches `Server`.
pub const SERVER: &str = "server";

impl Interface {
    /// The path of the exported value `name` to what `tag` is of, if the
    /// tag marks it.
    pub fn tagged(&self, tag: &str, name: &str) -> Option<&[String]> {
        let marked = self.tags.get(tag)?;
        (marked.iter().find(|(n, _)| n == name)).map(|(_, path)| &path[..])
    }

    pub fn find(&self, namespace: Namespace, name: &str) -> Option<&Export> {
        self.exports
            .iter()
            .find(|e| e.entity.namespace == namespace && e.entity.name == name)
    }

    /// Whether the module exports its own value `name`: a re-exported one
    /// is another module's, so it is not, say, this module's `main`.
    pub fn exports_value(&self, name: &str) -> bool {
        self.find(Namespace::Value, name)
            .is_some_and(|e| e.entity.module == self.module)
    }

    /// Exported members of type `parent`.
    pub fn members<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = &'a Export> {
        self.exports
            .iter()
            .filter(move |e| e.parent.as_deref() == Some(parent))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct File {
    iface_version: u32,
    /// Version of the compiler that wrote the file.
    compiler: String,
    #[serde(flatten)]
    interface: Interface,
}

/// The file contents: pretty JSON with a trailing newline.
pub fn write(interface: &Interface) -> String {
    let file = File {
        iface_version: IFACE_VERSION,
        compiler: crate::VERSION.to_owned(),
        interface: interface.clone(),
    };
    serde_json::to_string_pretty(&file).expect("an interface serializes") + "\n"
}

/// Reads the interface of `module`; `Err` explains why the file is unusable.
pub fn read(bytes: &[u8], module: &str) -> Result<Interface, String> {
    let file: File = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if file.iface_version != IFACE_VERSION {
        return Err(format!(
            "`iface-version` {} is not supported; this compiler reads {IFACE_VERSION}",
            file.iface_version
        ));
    }
    if file.compiler != crate::VERSION {
        return Err(format!(
            "it was written by flick {}, this is flick {}",
            file.compiler,
            crate::VERSION
        ));
    }
    if file.interface.module != module {
        return Err(format!(
            "it describes `{}`, not `{module}`",
            file.interface.module
        ));
    }
    Ok(file.interface)
}

/// Whether a count is left out of the interface file.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[cfg(test)]
mod tests;
