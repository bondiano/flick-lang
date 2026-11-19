//! What of a manifest the layers below the project read: its name, and its
//! `[target.<name>.dependencies]`, which an implicit package has too.

#[cfg(feature = "native-project")]
mod target;

#[cfg(feature = "native-project")]
pub(crate) use target::{RawTarget, target_dependencies, target_dependencies_in};

use crate::source::Span;

/// The manifest of a project or a package, in its root.
pub const CONFIG_FILE: &str = "flick.toml";

/// A package of the ecosystem of the backend `target` that a manifest
/// needs, under `[target.<target>.dependencies]`; that backend checks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDependency {
    pub target: String,
    pub name: String,
    /// A version or range, as the backend's ecosystem writes one.
    pub version: String,
    pub span: Span,
}
