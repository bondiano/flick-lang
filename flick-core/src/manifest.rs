//! What of a manifest the layers below the project read: its name, and its
//! `[js.dependencies]`, which an implicit package has too.

#[cfg(feature = "native-project")]
mod js;

#[cfg(feature = "native-project")]
pub(crate) use js::{RawJs, js_dependencies, js_dependencies_in};

use crate::source::Span;

/// The manifest of a project or a package, in its root.
pub const CONFIG_FILE: &str = "flick.toml";

/// An npm package a manifest needs at run time on the js target, under
/// `[js.dependencies]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsDependency {
    pub name: String,
    /// A version or range of the registry.
    pub version: String,
    pub span: Span,
}
