//! Constructors, type declarations, and how data values are represented.

use serde::{Deserialize, Serialize};

use crate::{Constraint, TypeRef};

/// A data, error or nominal type declared by a module, with its
/// layout-relevant type parameters and constructors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub name: String,
    /// Layout-relevant type parameters (effect, service, error, capture,
    /// region, symbol and constraint parameters are omitted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    /// Whether this declaration is a `type error`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub error: bool,
    /// Constructors in declaration order; empty for nominal/opaque types
    /// without constructors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constructors: Vec<ConstructorDecl>,
    /// For an `opaque type alias`, the underlying representation type in terms
    /// of `params` (so layout-aware backends can lay out opaque aliases).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<TypeRef>,
}

/// One constructor of a [`TypeDecl`], with its field types, record layout,
/// GADT existentials, dictionary constraints, and result arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructorDecl {
    pub name: String,
    pub rep: Rep,
    /// Quantified layout-relevant type parameters of this constructor.
    /// For an ordinary constructor, these match the enclosing
    /// [`TypeDecl::params`]; for a GADT constructor, they are the
    /// constructor's own quantified variables (indices and existentials).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<String>,
    /// The subset of `type_params` that are existential (not determined by
    /// the constructor's result type).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub existentials: Vec<String>,
    /// Class constraints whose dictionaries values of this constructor hold
    /// after the fields (one dictionary per constraint).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    /// Field types in declaration order, in terms of `type_params`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<TypeRef>,
    /// When the single field is declared as a closed record (matching
    /// `Module.records`), its labels in canonical UTF-16 order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<Vec<String>>,
    /// For a GADT constructor (`| K ... : T args`), the result type's
    /// layout-relevant arguments in terms of `type_params`. `None` for an
    /// ordinary constructor whose result is `T params`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_args: Option<Vec<TypeRef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constructor {
    pub module: String,
    pub ty: String,
    pub name: String,
    pub arity: usize,
    /// How its values are represented.
    pub rep: Rep,
    /// The dictionaries its values hold after the fields: one per class
    /// constraint of a constructor of a GADT.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dicts: usize,
    /// The field types of the constructor: over the constructor's type
    /// parameters on `ItemKind::Constructor`, and instantiated at a use
    /// (`ExprKind::Con`, `Pat::Con`) when available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<TypeRef>,
    /// Type arguments instantiating the constructor's quantified
    /// layout-relevant type parameters at a use (`ExprKind::Con`, `Pat::Con`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_args: Vec<TypeRef>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// How the values of one constructor are represented.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rep {
    #[default]
    Tagged,
    Unboxed,
    Typeof(JsClass),
    /// The constructor is its literal: a string or an integer.
    Literal(EnumLit),
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

/// The literal a constructor of an enum is: all of a type's are strings, or
/// all are integers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnumLit {
    Int(i32),
    String(String),
}

/// A type constructor by its module and name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeName {
    pub module: String,
    pub name: String,
}
