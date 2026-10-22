//! Shared Rust core for the Flick compiler and LSP.
//!
//! Source of truth: `Flick спецификация ML языка с HKT и Effect.md` in the
//! repository root. Implemented so far: diagnostics, syntax, `fmt`, names and
//! modules, and the project model.

pub mod bootstrap;
pub mod consteval;
pub mod diagnostics;
pub mod fmt;
pub mod iface;
pub mod project;
pub mod resolve;
pub mod source;
pub mod syntax;

/// Version of the Flick language this core targets (spec header: "Flick 0.7").
pub const LANGUAGE_VERSION: &str = "0.7";

/// Checks one source file on its own: syntax, and the names it defines and
/// exports; its imports are not loaded. A whole program goes through
/// [`project::check`].
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
    let (_, resolved) = resolve::resolve(&module, &|_| None);
    let mut diagnostics = parse.diagnostics;
    diagnostics.extend(resolved);
    diagnostics.sort_by_key(|d| d.span.map(|s| (s.start, s.end)));
    diagnostics
}
