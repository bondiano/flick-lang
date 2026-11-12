//! The lang items of a program: the built-in entities the compiler knows by
//! their role. A backend finds them here, not by their names, which the
//! library that declares them may change.

use serde::{Deserialize, Serialize};

/// Where the program declares one lang item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LangItem {
    /// The item's role, as `@lang` spells it: `option`, `some`, `int`.
    pub lang: String,
    pub module: String,
    pub namespace: Namespace,
    /// The entity's name; a module's own, for an item that is a module.
    pub name: String,
}

/// The names a lang item is among.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    Value,
    /// Types, classes, services, errors and modules.
    Type,
    Constructor,
}
