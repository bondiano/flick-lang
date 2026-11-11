//! What the checker found, as a plugin is told it: the types of the holes of
//! a block, the structure of its code, and the places of a module that name
//! what the plugin's package watches.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::Span;

/// A type the checker found, as it shows it and qualified with aliases
/// expanded; `exact` is none for a type with errors or still unknown parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactType {
    pub shown: String,
    pub exact: Option<String>,
    /// The exact base type of an enum, `String` or `Int`.
    pub literal_of: Option<String>,
}

/// A field of a record a fact's type is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactField {
    pub label: String,
    pub ty: FactType,
}

/// Source structure, independent of generated wrappers. Nesting entries are
/// the first nodes beyond the limit, in source order, without descendants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Structure {
    pub lines: u32,
    pub complexity: u32,
    pub nesting: Vec<Nesting>,
    pub max_lines: u32,
    pub max_complexity: u32,
    pub max_nesting: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nesting {
    pub span: Span,
    pub depth: u32,
}

/// The type of a `_` of a block's code, and its closed-record fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub span: Span,
    pub ty: FactType,
    pub fields: Option<Vec<FactField>>,
}

/// A value the compiler folded where a program writes it: the constant a
/// watched call is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constant {
    Number(f64),
    Text(String),
    Boolean(bool),
}

/// An argument of a watched call: where it is written, the type the checker
/// found for it, the fields of a closed record, and the constant it folds
/// to, if any.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Argument {
    pub span: Span,
    pub ty: FactType,
    pub fields: Option<Vec<FactField>>,
    pub constant: Option<Constant>,
    /// Written as a `fun`, maybe in parentheses: a function made where the
    /// call is, each time it runs.
    pub lambda: bool,
}

/// Read-only expression syntax. Children precede their parent, as in `Arena`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpressionFact {
    /// Shared by calls of the same predicate in a checked module.
    pub nodes: Arc<[ExpressionKind]>,
    pub root: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExpressionKind {
    Constant(Constant),
    /// An exact integer literal in decimal notation.
    BigInt(String),
    Reference {
        module: String,
        name: String,
    },
    Variable(String),
    Unary {
        op: String,
        value: u32,
    },
    Binary {
        op: String,
        left: u32,
        right: u32,
    },
    If {
        condition: u32,
        then: u32,
        otherwise: u32,
    },
    Lambda {
        parameter: String,
        body: u32,
    },
    Unknown,
}

/// Facts about a parse companion's type, without a compiler verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Refinement {
    Literals(Vec<Constant>),
    Predicate(ExpressionFact),
}

/// The enclosing top-level binding of a watched reference. These are
/// read-only syntax and scheme facts, not a verdict about the reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub module: String,
    pub name: Option<String>,
    pub span: Span,
    pub constrained: bool,
    pub in_function: bool,
    pub applied: u32,
    /// What the binding's function gives when all its arrows are applied,
    /// where its signature names the watched name.
    pub gives: Option<FactType>,
}

/// A place that calls a name the plugin's package watches: the module and
/// the name as the compiler knows them, where the call is written, its
/// arguments, and, for a `T$parse`, how `T` checks its value. Every span is
/// a byte offset in the text of the module's own file, and the diagnostics
/// of [`Plugin::check_calls`] are at spans of it too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallSite {
    pub module: String,
    pub name: String,
    pub span: Span,
    pub arguments: Vec<Argument>,
    pub refinement: Option<Refinement>,
    pub binding: Option<Binding>,
    /// The call is in code a plugin wrote, not in the module's own text.
    pub generated: bool,
    /// What marks the value named here, for a tag the package watches.
    pub tagged: Option<Tagged>,
    /// What the code here asks for that a function naming the watched
    /// service does not allow.
    pub lacking: Option<Lacking>,
}

/// Code that asks for more than the function expected at its place allows,
/// where that function's row names a service the package watches. The
/// compiler refuses it in its own words; an error the plugin gives at the
/// place's span stands for the compiler's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lacking {
    /// The function expected: its services and whether its row is open.
    pub allowed: Enclosing,
    /// What is not allowed, as the compiler's message words it: ``service
    /// `Write` ``, or ``services `Log`, `Clock` and error `Missing` ``.
    pub what: String,
    /// The services not allowed, as shown, and those of them with a name,
    /// qualified.
    pub services: Vec<String>,
    pub labels: Vec<String>,
    /// The errors not allowed, as shown.
    pub errors: Vec<String>,
}

/// A value a tag marks, where a module names it inside a function: `watch`
/// asks for the tag as `@tag`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tagged {
    pub tag: String,
    /// The value as the module writes it: `fresh`, or `State.fresh`.
    pub named: String,
    /// The value of the library the tag is written on, which `named` is or
    /// reaches.
    pub reaches: String,
    /// The functions around the place, innermost first, up to the nearest
    /// `task`: what runs later is not inside them.
    pub inside: Vec<Enclosing>,
}

/// A function around a place: the services its arrow names, and whether its
/// row is open to more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enclosing {
    pub services: Vec<String>,
    pub open: bool,
}
