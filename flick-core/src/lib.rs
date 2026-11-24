//! Shared Rust core for the Flick compiler and LSP.
//!
//! Source of truth: the language specification in `docs/src/content/docs/spec/`.

pub use flick_diagnostics::{diagnostics, source};
pub use flick_ffi::ffi;
pub use flick_syntax::syntax;

pub mod api_diff;
pub mod build;
mod clock;
pub mod codegen;
pub mod consteval;
pub mod doc;
pub mod effects;
pub mod fmt;
#[cfg(feature = "ide")]
pub mod ide;
pub mod iface;
pub mod implicit_packages;
pub mod ir;
pub mod logic;
pub mod manifest;
pub mod markdown;
pub mod numeric;
pub mod opt;
pub mod plugin;
pub mod project;
#[cfg(feature = "ide")]
pub mod projection;
pub mod resolve;
pub mod types;

#[cfg(test)]
mod builtin_source;
#[cfg(test)]
mod frontend_tests;
#[cfg(test)]
mod layer_tests;
/// Version of the compiler; `.iface` files record it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Checks one source file on its own: syntax, names and types; its imports
/// are not loaded, so names from them are taken on trust. A whole program goes
/// through [`project::check`].
pub fn check(sources: &source::SourceMap, file: source::FileId) -> Vec<diagnostics::Diagnostic> {
    let (parse, syntax, expansion, derived) = read(sources, file);
    project::check_file(sources, file, parse, syntax, expansion, derived)
}

/// Checks one source file on its own, also proving its function contracts in
/// `PureTotalV2`. A whole program goes through [`project::prove`].
#[cfg(feature = "contracts")]
pub fn prove(sources: &source::SourceMap, file: source::FileId) -> Vec<diagnostics::Diagnostic> {
    let (parse, syntax, expansion, derived) = read(sources, file);
    project::prove_file(sources, file, parse, syntax, expansion, derived)
}

/// Parses `file` and reads its module syntax, with the witnesses its
/// `deriving` names, which std's plugin writes.
fn read(
    sources: &source::SourceMap,
    file: source::FileId,
) -> (
    syntax::Parse,
    resolve::ModuleSyntax,
    Option<plugin::expansion::Expansion>,
    Vec<diagnostics::Diagnostic>,
) {
    let text = &sources.get(file).text;
    let parse = syntax::parse(text, file).into_module();
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let name = syntax.header.as_ref().map_or("", |h| h.name.as_str());
    let (expansion, derived) =
        plugin::derive::builtin_expansion(text, file, &parse.tree, name, &syntax);
    (parse, syntax, expansion, derived)
}
