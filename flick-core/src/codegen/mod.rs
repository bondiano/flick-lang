//! Native development adapter for the JavaScript emitter and the Go baseline.
//! Shared frontend projections stay in this crate; the public plugin ABI is WIT.

#[cfg(feature = "native-project")]
pub mod go;
#[cfg(feature = "native-project")]
pub mod sourcemap;
pub mod ts;

pub use flick_backend_js::codegen::{
    BUILTIN_DIR, EffectBridge, Known, Labels, Options, Output, Resolved, direct_methods,
    exported_worker, ident, instance_export, js, module_path, prefix_key, qualified_id, relative,
    tag,
};
pub(crate) use flick_backend_js::codegen::{RuntimeLayout, js_parts};
pub use flick_backend_support::tail;
pub(crate) use flick_backend_support::{exports, names};
#[cfg(feature = "native-project")]
pub(crate) use flick_backend_support::{loops, order};

#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
pub(crate) mod test_options;
#[cfg(test)]
mod tests;
