//! Native development adapter for the JavaScript emitter and the Go baseline.
//! Shared frontend projections stay in this crate; the public plugin ABI is WIT.

#[cfg(feature = "native-project")]
pub mod go;
#[cfg(feature = "native-project")]
pub mod sourcemap;
pub mod ts;

#[cfg(feature = "native-js")]
pub use flick_backend_js::codegen::{
    BUILTIN_DIR, EffectBridge, Known, Labels, Options, Output, Resolved, direct_methods,
    exported_worker, ident, instance_export, js, module_path, prefix_key, qualified_id, relative,
    tag,
};
#[cfg(feature = "native-js")]
pub(crate) use flick_backend_js::codegen::{RuntimeLayout, js_parts};
#[cfg(not(feature = "native-js"))]
pub use flick_backend_support::names::{ident, instance as instance_export, qualified_id, tag};
pub use flick_backend_support::tail;
#[cfg(not(feature = "native-js"))]
pub use flick_backend_support::{EffectBridge, Known};
pub(crate) use flick_backend_support::{exports, names};

#[cfg(not(feature = "native-js"))]
pub const BUILTIN_DIR: &str = "_flick";

#[cfg(not(feature = "native-js"))]
pub struct Resolved {
    pub module: String,
    pub spec: String,
    pub specifier: String,
}

#[cfg(not(feature = "native-js"))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Labels {
    #[default]
    All,
    Components,
    None,
}

#[cfg(not(feature = "native-js"))]
pub fn module_path(builtins: &[&str], typescript: bool, module: &str) -> String {
    let ext = if typescript { "ts" } else { "js" };
    let file = format!("{}.{ext}", module.replace('.', "/"));
    if builtins.contains(&module) {
        format!("{BUILTIN_DIR}/{file}")
    } else {
        file
    }
}

#[cfg(not(feature = "native-js"))]
pub fn prefix_key(module: &str, before: u32) -> String {
    format!("{module}$lexical${before}")
}

#[cfg(not(feature = "native-js"))]
pub fn relative(from: &str, to: &str) -> String {
    if let Some(bare) = to.strip_prefix("npm:") {
        return bare.to_owned();
    }
    let from_dir: Vec<&str> = from.split('/').collect();
    let from_dir = &from_dir[..from_dir.len() - 1];
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let up = from_dir.len() - common;
    let rest = to_parts[common..].join("/");
    if up == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(up))
    }
}

#[cfg(all(test, feature = "native-js"))]
mod pattern_tests;
#[cfg(all(test, feature = "native-js"))]
pub(crate) mod test_options;
#[cfg(all(test, feature = "native-js"))]
mod tests;
