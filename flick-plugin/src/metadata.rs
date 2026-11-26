//! Static plugin metadata (`PluginMetadata`, `HeadSpec`) and rich
//! declaration metadata (`DeriveTarget`, `CaseTarget`, `Visibility`,
//! `AttributeMeta`, `LiteralValue`).

use serde::{Deserialize, Serialize};

use crate::{Block, ConstraintMeta, Span, Written};

/// How much of a type declaration its module exports to callers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    /// Exported with its constructors (or transparent alias body).
    #[default]
    PublicConstructors,
    /// Exported by name only (`opaque type` or exported without `(..)`).
    Opaque,
    /// Not exported from its module.
    Private,
}

impl Visibility {
    /// Whether the type name itself is exported from its module.
    #[must_use]
    pub const fn is_public(self) -> bool {
        !matches!(self, Self::Private)
    }

    /// Whether the type's constructors or representation are exported.
    #[must_use]
    pub const fn exposes_constructors(self) -> bool {
        matches!(self, Self::PublicConstructors)
    }
}

/// An `@name args...` attribute attached to a type or variant.
///
/// `span` is relative to `DeriveTarget.decl` when the attribute sits inside
/// the declaration (on a variant), or `0..0` for attributes attached above
/// the declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttributeMeta {
    pub name: String,
    pub args: String,
    pub span: Span,
}

/// The decoded literal value of an enum constructor (`"GET"` or `-404`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LiteralValue {
    String(String),
    Int(String),
}

/// Syntactic classification of one block head declared by a plugin.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HeadSpec {
    pub head: String,
    pub module: String,
    pub bare: bool,
    pub declarations: Vec<String>,
    pub named: bool,
    pub top: bool,
    pub bracket_args: bool,
}

/// Static metadata declared by a plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub heads: Vec<HeadSpec>,
    pub watch: Vec<String>,
    pub derives: Vec<String>,
    /// Compiler-owned diagnostic codes (honored only for first-party `std`).
    pub own_codes: Vec<String>,
}

/// A type a class's derive makes an instance for: its declaration as a
/// block, a declaration of the module, and what it declares, at spans of
/// that block. A type of another module (`deriving instance`) is not
/// `local`: its spans are no holes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeriveTarget {
    pub decl: Block,
    /// Its name, qualified for a type of another module: `Other.Color`.
    pub name: String,
    /// Span of the type's name token within `decl`.
    pub name_span: Span,
    pub params: Vec<String>,
    /// Span of each type parameter within `decl`.
    pub param_spans: Vec<Span>,
    /// The kind each parameter is written with, `Type -> Type` of
    /// `(f : Type -> Type)`, empty without one.
    pub kinds: Vec<String>,
    /// Its constructors, qualified as its name is, with their fields' types.
    pub cases: Vec<CaseTarget>,
    /// The type a `type alias` stands for.
    pub alias: Option<Written>,
    /// The type its values are made of, for a type over another whose
    /// constructor is its declaration's alone: a `refined type`, or an
    /// `unboxed type` of a block with a constructor only generated code can
    /// name. Its values come from `T.parse` and are read by `T.value`.
    pub wraps: Option<Written>,
    /// The words before its name: `type`, `alias`, `opaque`, `unboxed`,
    /// `untagged`, `error`.
    pub modifiers: Vec<String>,
    /// The classes its `deriving` names, as written: `Schema`, `Cli.Value`.
    pub deriving: Vec<String>,
    /// Span of each `deriving` class name within `decl`.
    pub deriving_spans: Vec<Span>,
    /// Export visibility of this type and its constructors.
    pub visibility: Visibility,
    /// Attributes attached to this type declaration.
    pub attributes: Vec<AttributeMeta>,
    pub local: bool,
}

/// A constructor of a derive's target, its spans, documentation, attributes,
/// constraints, field types, optional GADT result type and enum literal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaseTarget {
    pub name: String,
    /// Span of the variant declaration within `DeriveTarget.decl`.
    pub span: Span,
    /// Span of the constructor name within `DeriveTarget.decl`.
    pub name_span: Span,
    /// `///` doc comments directly above this variant.
    pub docs: String,
    /// Attributes attached to this variant.
    pub attributes: Vec<AttributeMeta>,
    /// Existential/GADT class constraints on this constructor.
    pub constraints: Vec<ConstraintMeta>,
    pub fields: Vec<Written>,
    /// Explicit GADT result type (`: Expr Int`), if written.
    pub result: Option<Written>,
    pub literal: Option<Span>,
    /// Decoded literal value of an enum constructor, if written.
    pub literal_value: Option<LiteralValue>,
    #[serde(default)]
    pub refined: bool,
    #[serde(default)]
    pub constrained: bool,
}
