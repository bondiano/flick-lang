//! Module export tables carried by the portable IR: exposed values,
//! re-exports, and host-named `@export "name"` functions.

use serde::{Deserialize, Serialize};

use crate::{Namespace, Projection, Span};

/// The exports of a module: its own values, which other modules and the
/// host call, what it exports of other modules, and functions explicitly
/// exported under host names via `@export "name"`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exports {
    /// The exported values of the module itself.
    pub values: Vec<Exported>,
    /// The entities of other modules the module exports.
    pub reexports: Vec<Reexport>,
    /// Top-level functions explicitly exported under host names via `@export`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host: Vec<HostExport>,
}

impl Exports {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.reexports.is_empty() && self.host.is_empty()
    }
}

/// An exported value of a module itself: its name, the arrows a call
/// supplies when its last stage is Sync, and the fields it reads when it
/// is a projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exported {
    pub name: String,
    pub sync: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<Projection>,
}

/// An entity of another module that a module exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reexport {
    pub module: String,
    pub namespace: Namespace,
    pub name: String,
}

/// A top-level function exported to the host under an explicit name via
/// `@export "name"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostExport {
    /// The Flick binding name in this module.
    pub name: String,
    /// The host export name written in `@export "…"`.
    pub export_name: String,
    /// Source span of the `@export "…"` attribute.
    pub span: Span,
}
