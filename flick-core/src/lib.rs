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
pub mod implicit_packages;
pub mod ir;
pub mod markdown;
pub mod numeric;
pub mod opt;
pub mod plugin;
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
    let name = syntax.header.as_ref().map_or("", |h| h.name.as_str());
    // the witnesses its `deriving` names, which std's plugin writes
    let (expansion, derived) =
        plugin::derive::builtin_expansion(text, file, &parse.tree, name, &syntax);
    let module = resolve::Module {
        name,
        file,
        text: expansion.as_ref().map_or(text, |e| &e.text),
        tree: expansion.as_ref().map_or(&parse.tree, |e| &e.tree),
        syntax: &syntax,
        generated: expansion.as_ref().map_or(resolve::NO_GENERATED, |e| e.len),
    };
    // a lone file imports only built-in modules
    let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
    let (mut interface, resolved) = resolve::resolve(&module, &builtin);
    let local = |name: &str| name == module.name;
    // std's plugin is linked into the compiler, so a lone file is watched
    // for its names without a manifest
    let watched = types::Watched::new(plugin::first_party::WATCH);
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
        watched: &watched,
        stages_by_class: false,
    };
    let mut diagnostics = parse.diagnostics;
    diagnostics.extend(derived);
    diagnostics.extend(resolved);
    let (mut checked, typing) = types::check_typed(
        &module,
        &builtin,
        types::builtins(),
        options,
        &mut interface,
    );
    checked.extend(watched_calls(&typing, expansion.as_ref(), file, &watched));
    diagnostics.extend(types::beside_syntax_errors(&parse.tree, checked));
    if let Some(expansion) = &expansion {
        expansion.restore(&mut diagnostics, None);
    }
    diagnostics.sort_by_key(|d| d.span.map(|s| (s.start, s.end)));
    diagnostics
}

/// What std's plugin finds wrong with the places of the file that call the
/// names it watches; a lone file has no group of blocks, so no state.
fn watched_calls(
    typing: &types::Typing,
    expansion: Option<&plugin::expansion::Expansion>,
    file: source::FileId,
    watched: &types::Watched,
) -> Vec<diagnostics::Diagnostic> {
    let package = plugin::first_party::PACKAGE;
    let Some(std) = plugin::first_party::plugin(package) else {
        return Vec::new();
    };
    let sites = plugin::calls::placed_sites(&typing.call_sites, expansion, file);
    plugin::calls::check_calls(std.as_ref(), package, watched, &sites, &[], file)
}
