//! The debug (reference) Effect backend: ESM from the typed IR, with source
//! maps. Pure code is plain JS; a runtime stage is an `Effect.gen` whose
//! runtime calls are `yield*`; data follows the ABI in
//! `docs/src/content/docs/reference/codegen.md`.

pub mod go;
mod js;
mod names;
pub mod sourcemap;
pub mod tail;
pub mod ts;

pub use js::{direct_methods, exported_worker, js, relative};
pub use names::{ident, instance as instance_export, qualified_id, tag};

use crate::iface::Interface;
use crate::source::Span;

/// Where modules are and who owns them, as the build lays them out.
pub struct Options<'a> {
    /// The output path of a module, relative to the output directory:
    /// `App/Users.js`.
    pub path: &'a dyn Fn(&str) -> String,
    /// The package a module belongs to, for its constructor ids; `None` for
    /// the built-in modules and a module outside any package.
    pub package: &'a dyn Fn(&str) -> Option<String>,
    /// The import specifier of a foreign module `spec` named in `module`.
    pub foreign: &'a dyn Fn(&str, &str) -> String,
    /// Emit TypeScript (`--emit ts`) instead of JavaScript.
    pub typescript: bool,
    /// The interface of a module, this one included: TypeScript output
    /// declares the exported types and annotates the exported values.
    pub interface: &'a dyn Fn(&str) -> Option<Interface>,
    /// The release profile: worker functions for known full applications
    /// and direct methods of canonical instances.
    pub release: bool,
    /// In release, the arity `n` of the worker `name$n` that a module,
    /// this one included, exports for its value `name` (see
    /// [`exported_worker`]); a full application calls it.
    pub workers: &'a dyn Fn(&str, &str) -> Option<usize>,
    /// In release, whether a module exports the method of its instance as
    /// a constant (see [`direct_methods`]): by module, instance and method.
    pub direct: &'a dyn Fn(&str, &str, &str) -> bool,
    /// In release, which top-level values keep their label; debug labels
    /// every atom, view and action.
    pub labels: Labels,
}

/// `[build] labels`: the top-level values a release build labels
/// `Module.name` for debugging tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Labels {
    /// Atoms, views and actions.
    #[default]
    All,
    /// Views and actions; an atom's label costs a stack trace when it is made.
    Components,
    None,
}

/// A compiled module.
#[derive(Debug)]
pub struct Output {
    pub code: String,
    /// Each generated position, 0-based line and UTF-16 column, with the
    /// source node it comes from and, at the name of a top-level value, its
    /// Flick name.
    pub mappings: Vec<(u32, u32, Span, Option<String>)>,
    /// The Flick modules the code imports, types included.
    pub imports: Vec<String>,
}

#[cfg(test)]
mod tests;
