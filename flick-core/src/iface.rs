//! `.iface`, the native interface of a module: versioned JSON that an
//! importer reads instead of the module's source. Schema:
//! `docs/src/content/docs/reference/iface.md`: names and their identity, the types and kinds
//! of exports with their captures, classes and instances, and from a build
//! the data and dictionary ABI, summaries of values and fingerprints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::{Instance, Type, TypeDef};

/// Bumped on every change of the schema.
pub const IFACE_VERSION: u32 = 1;

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
    Refined,
    /// `type T` without a definition
    Abstract,
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
    pub exports: Vec<Export>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<Hidden>,
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
    /// The exported functions that make an atom, an action or a view, which
    /// a render must not call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub makers: Vec<String>,
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
    // where declarations are written changes nothing for importers
    bare.source = None;
    for export in &mut bare.exports {
        export.at = None;
    }
    let json = serde_json::to_string(&bare).expect("an interface serializes");
    let hash = json.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

impl Interface {
    pub fn find(&self, namespace: Namespace, name: &str) -> Option<&Export> {
        self.exports
            .iter()
            .find(|e| e.entity.namespace == namespace && e.entity.name == name)
    }

    pub fn exports_value(&self, name: &str) -> bool {
        self.find(Namespace::Value, name).is_some()
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
    /// Flick language version of the compiler that wrote the file.
    language: String,
    #[serde(flatten)]
    interface: Interface,
}

/// The file contents: pretty JSON with a trailing newline.
pub fn write(interface: &Interface) -> String {
    let file = File {
        iface_version: IFACE_VERSION,
        language: crate::LANGUAGE_VERSION.to_owned(),
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
    if file.language != crate::LANGUAGE_VERSION {
        return Err(format!(
            "it was written for Flick {}, this compiler is Flick {}",
            file.language,
            crate::LANGUAGE_VERSION
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{self, Module};
    use crate::source::SourceMap;
    use crate::{syntax, types};

    fn interface_of(text: &str) -> Interface {
        let mut sources = SourceMap::default();
        let file = sources.add("t.fl", text);
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let module = Module {
            name: "App.Shapes",
            file,
            text,
            tree: &parse.tree,
            syntax: &syntax,
        };
        let (mut interface, mut diagnostics) = resolve::resolve(&module, &|_| None);
        let local = |name: &str| name == "App.Shapes";
        let options = crate::types::Options {
            local: &local,
            limits: types::Limits::default(),
            sql: Default::default(),
            no_let_assert: false,
        };
        diagnostics.extend(types::check(
            &module,
            &|_| None,
            types::builtins(),
            options,
            &mut interface,
        ));
        diagnostics.retain(|d| !d.unnecessary());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        interface
    }

    #[test]
    fn schema_is_pinned_and_round_trips() {
        let interface = interface_of(
            "module App.Shapes exposing (Shape(..), Id, Size, Mode(..), Show(..), Collection(..), area, now, Pair, Clock, NotFound(..), Port, Box, wrap, later)\n\
             type Shape = Circle Number | Square Number\n  deriving Eq\n\
             opaque type Id = Id String\n\
             type alias Size = Number\n\
             kind Mode = Small | Big\n\
             class Show a =\n  show : a -> String\n\
             class Eq a => Collection c a | c -> a =\n  empty : c\n\
             instance Show Shape =\n  show : Shape -> String\n  let show s = \"shape\"\n\
             area : Shape -> Number\nlet area s = 0\n\
             now : Unit ->{} Number\nforeign sync now from global \"Date.now\"\n\
             type Pair\n\
             service Clock =\n  tick : Unit ->{} Number\n\
             type error NotFound = NotFound String\n\
             refined type Port = Number where fun n -> n > 0\n\
             type alias Box (f : Type -> Type) a = { value : f a; size : Size }\n\
             wrap : f a -> Box f a\nlet wrap value = { value = value; size = 1 }\n\
             later : forall s. Handle s -> (Unit -> Handle s)^{s}\nlet later h = fun () -> h\n\
             type Handle (s : Region)\n",
        );
        let json = write(&interface);
        insta::assert_snapshot!(json);
        let back = read(json.as_bytes(), "App.Shapes").expect("valid");
        assert_eq!(write(&back), json);
    }

    #[test]
    fn rejects_other_versions_and_modules() {
        let json = write(&Interface {
            module: "A".into(),
            ..Interface::default()
        });
        assert!(
            read(json.as_bytes(), "B")
                .expect_err("rejected")
                .contains("describes `A`")
        );
        let current = format!("\"iface-version\": {IFACE_VERSION}");
        let future = json.replace(&current, "\"iface-version\": 7");
        assert!(
            read(future.as_bytes(), "A")
                .expect_err("rejected")
                .contains("`iface-version` 7")
        );
        let other = json.replace(crate::LANGUAGE_VERSION, "9.9");
        assert!(
            read(other.as_bytes(), "A")
                .expect_err("rejected")
                .contains("Flick 9.9")
        );
        assert!(read(b"{", "A").is_err());
    }
}
