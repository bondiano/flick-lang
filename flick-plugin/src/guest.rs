//! The glue of a plugin built as a WebAssembly component:
//! `flick_plugin::export!(MyPlugin);` in the plugin's crate exports a
//! [`Plugin`] value through the world of `wit/plugin.wit`, and the compiler's
//! calls come back to it as a [`Compiler`].

use crate::{
    Compiler, Completion, Diagnostic, Hole, Namespace, Plugin, ReachError, Request, Response,
};

#[allow(
    clippy::same_length_and_capacity,
    clippy::too_many_arguments,
    missing_docs
)]
#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "plugin",
        path: "wit",
        pub_export_macro: true,
        export_macro_name: "export_component",
        default_bindings_module: "flick_plugin::guest::bindings",
    });
}

crate::wit_conversions!(bindings::flick::plugin::types);
crate::reach_conversions!(bindings::flick::plugin::types);

use bindings::flick::plugin::{compiler, types};

/// The compiler, through the imports of the component.
struct Imports;

impl Compiler for Imports {
    fn parse_expr(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        compiler::parse_expr(start, end)
            .map(convert::from_hole)
            .map_err(convert::from_diagnostic)
    }

    fn parse_type(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        compiler::parse_type(start, end)
            .map(convert::from_hole)
            .map_err(convert::from_diagnostic)
    }

    fn parse_pat(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        compiler::parse_pat(start, end)
            .map(convert::from_hole)
            .map_err(convert::from_diagnostic)
    }

    fn parse_decl(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        compiler::parse_decl(start, end)
            .map(convert::from_hole)
            .map_err(convert::from_diagnostic)
    }

    fn parse_body(&mut self, start: u32, end: u32) -> Result<Hole, Diagnostic> {
        compiler::parse_body(start, end)
            .map(convert::from_hole)
            .map_err(convert::from_diagnostic)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, ReachError> {
        compiler::read_file(path).map_err(reach_convert::from_error)
    }

    fn env(&mut self, name: &str) -> Result<Option<String>, ReachError> {
        compiler::env(name).map_err(reach_convert::from_error)
    }

    fn literal_of(&mut self, name: &str) -> Option<String> {
        compiler::literal_of(name)
    }

    fn published(&mut self, name: &str) -> Option<Vec<u8>> {
        compiler::published(name)
    }

    fn published_names(&mut self) -> Vec<String> {
        compiler::published_names()
    }

    fn in_scope(&mut self, namespace: Namespace, fits: Option<&str>) -> Vec<Completion> {
        let namespace = match namespace {
            Namespace::Value => types::Namespace::Value,
            Namespace::Type => types::Namespace::Type,
        };
        (compiler::in_scope(namespace, fits).into_iter())
            .map(convert::from_completion)
            .collect()
    }

    fn fetch(&mut self, request: &Request) -> Result<Response, ReachError> {
        compiler::fetch(&reach_convert::to_request(request))
            .map(reach_convert::from_response)
            .map_err(reach_convert::from_error)
    }
}

#[doc(hidden)]
pub fn collect(plugin: &impl Plugin, blocks: Vec<types::Block>) -> Vec<u8> {
    let blocks: Vec<_> = blocks.into_iter().map(convert::from_block).collect();
    plugin.collect(&blocks, &mut Imports)
}

#[doc(hidden)]
pub fn publish(plugin: &impl Plugin, state: &[u8]) -> Vec<types::Published> {
    (plugin.publish(state).into_iter())
        .map(|p| types::Published {
            module: p.module,
            name: p.name,
            data: p.data,
        })
        .collect()
}

#[doc(hidden)]
pub fn transform(
    plugin: &impl Plugin,
    program: Vec<u8>,
) -> Result<Vec<u8>, Vec<types::Diagnostic>> {
    (plugin.transform(program))
        .map_err(|found| found.into_iter().map(convert::to_diagnostic).collect())
}

#[doc(hidden)]
pub fn emit(
    plugin: &impl Plugin,
    artifacts: Vec<types::Artifact>,
) -> Result<Vec<types::Artifact>, Vec<types::Diagnostic>> {
    let artifacts = artifacts.into_iter().map(convert::from_artifact).collect();
    plugin
        .emit(artifacts)
        .map(|written| written.into_iter().map(convert::to_artifact).collect())
        .map_err(|found| found.into_iter().map(convert::to_diagnostic).collect())
}

#[doc(hidden)]
pub fn load(
    plugin: &impl Plugin,
    path: &str,
    bytes: &[u8],
) -> Result<types::Arena, Vec<types::Diagnostic>> {
    plugin
        .load(path, bytes)
        .map(convert::to_arena)
        .map_err(|found| found.into_iter().map(convert::to_diagnostic).collect())
}

#[doc(hidden)]
pub fn expand(
    plugin: &impl Plugin,
    block: types::Block,
    state: &[u8],
) -> Result<types::Arena, Vec<types::Diagnostic>> {
    plugin
        .expand(&convert::from_block(block), state, &mut Imports)
        .map(convert::to_arena)
        .map_err(|found| found.into_iter().map(convert::to_diagnostic).collect())
}

#[doc(hidden)]
pub fn complete(
    plugin: &impl Plugin,
    block: types::Block,
    state: &[u8],
    offset: u32,
    member: bool,
) -> Vec<types::Completion> {
    (plugin
        .complete(
            &convert::from_block(block),
            state,
            offset,
            member,
            &mut Imports,
        )
        .into_iter())
    .map(convert::to_completion)
    .collect()
}

#[doc(hidden)]
pub fn hover(
    plugin: &impl Plugin,
    block: types::Block,
    state: &[u8],
    offset: u32,
    facts: Vec<types::Fact>,
) -> Option<types::Hover> {
    let facts: Vec<_> = facts.into_iter().map(convert::from_fact).collect();
    (plugin.hover(&convert::from_block(block), state, offset, &facts)).map(convert::to_hover)
}

#[doc(hidden)]
pub fn highlight(plugin: &impl Plugin, block: types::Block, state: &[u8]) -> Vec<types::Highlight> {
    (plugin
        .highlight(&convert::from_block(block), state)
        .into_iter())
    .map(convert::to_highlight)
    .collect()
}

#[doc(hidden)]
pub fn format(
    plugin: &impl Plugin,
    block: types::Block,
    state: &[u8],
) -> Option<Vec<types::Piece>> {
    let pieces = plugin.format(&convert::from_block(block), state, &mut Imports)?;
    Some(pieces.into_iter().map(convert::to_piece).collect())
}

#[doc(hidden)]
pub fn derive(
    plugin: &impl Plugin,
    target: types::DeriveTarget,
    class: &str,
    args: Option<types::Block>,
    module: Vec<types::DeriveTarget>,
    state: &[u8],
) -> Result<types::Arena, Vec<types::Diagnostic>> {
    let module: Vec<_> = module.into_iter().map(convert::from_target).collect();
    let args = args.map(convert::from_block);
    let target = convert::from_target(target);
    plugin
        .derive(&target, class, args.as_ref(), &module, state, &mut Imports)
        .map(convert::to_arena)
        .map_err(|found| found.into_iter().map(convert::to_diagnostic).collect())
}

#[doc(hidden)]
pub fn check_structure(
    plugin: &impl Plugin,
    block: types::Block,
    facts: types::Structure,
) -> Vec<types::Diagnostic> {
    plugin
        .check_structure(&convert::from_block(block), &convert::from_structure(facts))
        .into_iter()
        .map(convert::to_diagnostic)
        .collect()
}

#[doc(hidden)]
pub fn check(
    plugin: &impl Plugin,
    block: types::Block,
    state: &[u8],
    facts: Vec<types::Fact>,
) -> Vec<types::Diagnostic> {
    let facts: Vec<_> = facts.into_iter().map(convert::from_fact).collect();
    (plugin
        .check(&convert::from_block(block), state, &facts)
        .into_iter())
    .map(convert::to_diagnostic)
    .collect()
}

#[doc(hidden)]
pub fn check_calls(
    plugin: &impl Plugin,
    calls: Vec<types::CallSite>,
    state: &[u8],
) -> Vec<types::Diagnostic> {
    let calls: Vec<_> = calls.into_iter().map(convert::from_call_site).collect();
    (plugin.check_calls(&calls, state).into_iter())
        .map(convert::to_diagnostic)
        .collect()
}

/// Exports the plugin `$plugin`, a value of a type implementing [`Plugin`],
/// as the component a crate of type `cdylib` builds for `wasm32-wasip2`.
#[macro_export]
macro_rules! export {
    ($plugin:expr) => {
        struct FlickPluginComponent;

        impl $crate::guest::bindings::Guest for FlickPluginComponent {
            fn collect(
                blocks: ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Block>,
            ) -> ::std::vec::Vec<u8> {
                $crate::guest::collect(&$plugin, blocks)
            }

            fn publish(
                state: ::std::vec::Vec<u8>,
            ) -> ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Published> {
                $crate::guest::publish(&$plugin, &state)
            }

            fn transform(
                program: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<
                ::std::vec::Vec<u8>,
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic>,
            > {
                $crate::guest::transform(&$plugin, program)
            }

            fn emit(
                artifacts: ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Artifact>,
            ) -> ::std::result::Result<
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Artifact>,
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic>,
            > {
                $crate::guest::emit(&$plugin, artifacts)
            }

            fn load(
                path: ::std::string::String,
                bytes: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<
                $crate::guest::bindings::flick::plugin::types::Arena,
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic>,
            > {
                $crate::guest::load(&$plugin, &path, &bytes)
            }

            fn expand(
                block: $crate::guest::bindings::flick::plugin::types::Block,
                state: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<
                $crate::guest::bindings::flick::plugin::types::Arena,
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic>,
            > {
                $crate::guest::expand(&$plugin, block, &state)
            }

            fn complete(
                block: $crate::guest::bindings::flick::plugin::types::Block,
                state: ::std::vec::Vec<u8>,
                offset: u32,
                member: bool,
            ) -> ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Completion> {
                $crate::guest::complete(&$plugin, block, &state, offset, member)
            }

            fn hover(
                block: $crate::guest::bindings::flick::plugin::types::Block,
                state: ::std::vec::Vec<u8>,
                offset: u32,
                facts: ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Fact>,
            ) -> ::std::option::Option<$crate::guest::bindings::flick::plugin::types::Hover> {
                $crate::guest::hover(&$plugin, block, &state, offset, facts)
            }

            fn highlight(
                block: $crate::guest::bindings::flick::plugin::types::Block,
                state: ::std::vec::Vec<u8>,
            ) -> ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Highlight> {
                $crate::guest::highlight(&$plugin, block, &state)
            }

            fn format(
                block: $crate::guest::bindings::flick::plugin::types::Block,
                state: ::std::vec::Vec<u8>,
            ) -> ::std::option::Option<
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Piece>,
            > {
                $crate::guest::format(&$plugin, block, &state)
            }

            fn derive(
                target: $crate::guest::bindings::flick::plugin::types::DeriveTarget,
                class: ::std::string::String,
                args: ::std::option::Option<$crate::guest::bindings::flick::plugin::types::Block>,
                module: ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::DeriveTarget>,
                state: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<
                $crate::guest::bindings::flick::plugin::types::Arena,
                ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic>,
            > {
                $crate::guest::derive(&$plugin, target, &class, args, module, &state)
            }

            fn check_structure(block: $crate::guest::bindings::flick::plugin::types::Block, facts: $crate::guest::bindings::flick::plugin::types::Structure) -> ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic> {
                $crate::guest::check_structure(&$plugin, block, facts)
            }

            fn check(
                block: $crate::guest::bindings::flick::plugin::types::Block,
                state: ::std::vec::Vec<u8>,
                facts: ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Fact>,
            ) -> ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic> {
                $crate::guest::check(&$plugin, block, &state, facts)
            }

            fn check_calls(
                calls: ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::CallSite>,
                state: ::std::vec::Vec<u8>,
            ) -> ::std::vec::Vec<$crate::guest::bindings::flick::plugin::types::Diagnostic> {
                $crate::guest::check_calls(&$plugin, calls, &state)
            }
        }

        $crate::guest::bindings::export_component!(FlickPluginComponent with_types_in $crate::guest::bindings);
    };
}
