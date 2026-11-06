//! Instances of classes, as code generation builds their dictionaries.

use serde::{Deserialize, Serialize};

use crate::{Constructor, Dict};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub module: String,
    /// The head as written: what `Dict::Instance` names it by.
    pub name: String,
    pub class: (String, String),
    /// The `instance` declaration, or the `deriving` clause: a byte range
    /// of the module's file.
    pub span: (u32, u32),
    /// The context, as its dictionaries are named.
    pub context: Vec<String>,
    pub supers: Vec<(String, Dict)>,
    /// For a derived instance: the dictionary of each field, constructor by
    /// constructor, and the constructors (`None` for a type without variants,
    /// whose one field is its representation).
    pub derived: Option<(Vec<Dict>, Option<Vec<Constructor>>)>,
    /// Methods taken from the class's defaults, with how many dictionaries
    /// of their own each takes, and the dictionaries its default signature
    /// needs, which it takes after the class's.
    pub defaults: Vec<(String, usize, Vec<Dict>)>,
    /// For `deriving Generic`: what its methods convert, which lowering
    /// writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic: Option<GenericInfo>,
    /// For the instance of `Enum` of an enum, the enum: its methods are the
    /// enum's companions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enumerates: Option<String>,
}

/// The constructors of a type that derives `Generic`, each with the labels
/// of its one record field when it has one, and the constructors of the
/// representation: `Inl` and `Inr` of `Sum`, `Con` and `Field`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericInfo {
    pub cases: Vec<(Constructor, Option<Vec<String>>)>,
    pub inl: Constructor,
    pub inr: Constructor,
    pub con: Constructor,
    pub field: Constructor,
}
