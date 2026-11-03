//! Shared Rust core for the Flick compiler and LSP.
//!
//! Source of truth: the language specification in `docs/src/content/docs/spec/`.

pub mod api_diff;
pub mod build;
mod clock;
pub mod codegen;
pub mod consteval;
pub mod diagnostics;
pub mod doc;
pub mod ffi;
pub mod fmt;
#[cfg(feature = "ide")]
pub mod ide;
pub mod iface;
pub mod ir;
pub mod numeric;
pub mod opt;
pub mod project;
#[cfg(feature = "ide")]
pub mod projection;
pub mod resolve;
pub mod source;
pub mod syntax;
pub mod types;

#[cfg(test)]
mod builtin_source;
/// Version of the compiler; `.iface` files record it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Checks one source file on its own: syntax, names and types; its imports
/// are not loaded, so names from them are taken on trust. A whole program goes
/// through [`project::check`].
pub fn check(sources: &source::SourceMap, file: source::FileId) -> Vec<diagnostics::Diagnostic> {
    let text = &sources.get(file).text;
    let parse = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let module = resolve::Module {
        name: syntax.header.as_ref().map_or("", |h| h.name.as_str()),
        file,
        text,
        tree: &parse.tree,
        syntax: &syntax,
    };
    // a lone file imports only built-in modules
    let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
    let (mut interface, resolved) = resolve::resolve(&module, &builtin);
    let local = |name: &str| name == module.name;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        sql: Default::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
    };
    let mut diagnostics = parse.diagnostics;
    diagnostics.extend(resolved);
    let checked = types::check(
        &module,
        &builtin,
        types::builtins(),
        options,
        &mut interface,
    );
    diagnostics.extend(types::beside_syntax_errors(&parse.tree, checked));
    diagnostics.sort_by_key(|d| d.span.map(|s| (s.start, s.end)));
    diagnostics
}
