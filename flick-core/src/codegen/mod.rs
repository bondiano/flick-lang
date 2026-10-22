//! The debug (reference) Effect backend: ESM from the typed IR, with source
//! maps. Pure code is plain JS; a runtime stage is an `Effect.gen` whose
//! runtime calls are `yield*`; data follows the ABI in
//! `docs/reference/codegen.md`.

mod js;
mod names;
pub mod sourcemap;

pub use js::js;
pub use names::tag;

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
}

/// A compiled module.
#[derive(Debug)]
pub struct Output {
    pub code: String,
    /// Each generated position, 0-based line and UTF-16 column, with the
    /// source node it comes from.
    pub mappings: Vec<(u32, u32, Span)>,
}

#[cfg(test)]
mod tests;
