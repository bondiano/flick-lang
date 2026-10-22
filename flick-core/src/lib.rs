//! Shared Rust core for the Flick compiler and LSP.
//!
//! Source of truth: `Flick спецификация ML языка с HKT и Effect.md` in the
//! repository root. Implemented so far: diagnostics, syntax, `fmt`, names and
//! modules, the project model, kinds and types, and classes.

pub mod bootstrap;
pub mod consteval;
pub mod diagnostics;
pub mod fmt;
pub mod iface;
pub mod ir;
pub mod project;
pub mod resolve;
pub mod source;
pub mod syntax;
pub mod types;

/// Version of the Flick language this core targets (spec header: "Flick 0.7").
pub const LANGUAGE_VERSION: &str = "0.7";

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
    let (mut interface, resolved) = resolve::resolve(&module, &|_| None);
    let local = |name: &str| name == module.name;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
    };
    let mut diagnostics = parse.diagnostics;
    diagnostics.extend(resolved);
    if !diagnostics
        .iter()
        .any(|d| d.code.area == diagnostics::codes::Area::Syntax)
    {
        diagnostics.extend(types::check(
            &module,
            &|_| None,
            types::builtins(),
            options,
            &mut interface,
        ));
    }
    diagnostics.sort_by_key(|d| d.span.map(|s| (s.start, s.end)));
    diagnostics
}
