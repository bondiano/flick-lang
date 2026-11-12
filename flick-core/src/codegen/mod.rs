//! The debug (reference) Effect backend: ESM from the typed IR, with source
//! maps. Pure code is plain JS; a runtime stage is an `Effect.gen` whose
//! runtime calls are `yield*`; data follows the ABI in
//! `docs/src/content/docs/reference/codegen.md`.

pub(crate) mod exports;
#[cfg(feature = "native-project")]
pub mod go;
mod js;
mod lexical;
mod loops;
pub(crate) use lexical::{RuntimeLayout, js_parts};
mod names;
#[cfg(feature = "native-project")]
pub mod sourcemap;
pub mod tail;
pub mod ts;

pub use js::{direct_methods, exported_worker, js, relative};
pub use names::{ident, instance as instance_export, qualified_id, tag};

/// Whether an RT build may use the separately installed Effect adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EffectBridge {
    #[default]
    Absent,
    Available,
    /// Opaque UI values and registries belong to the Effect adapter.
    AvailableWithUi,
    /// Procedures, groups and their calls belong to the Effect adapter.
    AvailableWithRpc,
    /// Both UI and RPC values belong to the Effect adapter.
    AvailableWithUiAndRpc,
}

impl EffectBridge {
    pub const fn available(self) -> bool {
        !matches!(self, Self::Absent)
    }

    /// The available bridge with the families whose values the adapter owns.
    pub const fn owning(ui: bool, rpc: bool) -> Self {
        match (ui, rpc) {
            (false, false) => Self::Available,
            (true, false) => Self::AvailableWithUi,
            (false, true) => Self::AvailableWithRpc,
            (true, true) => Self::AvailableWithUiAndRpc,
        }
    }

    pub const fn ui(self) -> bool {
        matches!(self, Self::AvailableWithUi | Self::AvailableWithUiAndRpc)
    }

    pub const fn rpc(self) -> bool {
        matches!(self, Self::AvailableWithRpc | Self::AvailableWithUiAndRpc)
    }
}

use crate::iface::Interface;
#[cfg(feature = "native-project")]
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
    /// Compile for Flick's own runtime instead of Effect (see `js::kernel`).
    pub rt: bool,
    /// Whether RT code may import the optional Effect adapter.
    pub effect_bridge: EffectBridge,
}

impl Options<'_> {
    pub(crate) fn typescript(&self) -> bool {
        self.typescript
    }
}

/// `[build] labels`: the top-level values a release build labels
/// `Module.name` for debugging tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Labels {
    /// Atoms, views and actions.
    #[default]
    All,
    /// Those of kind `component`, views and actions; an atom's label
    /// costs a stack trace when it is made.
    Components,
    None,
}

/// A compiled module.
#[derive(Debug)]
pub struct Output {
    pub code: String,
    #[cfg(feature = "native-project")]
    /// Each generated position, 0-based line and UTF-16 column, with the
    /// source node it comes from and, at the name of a top-level value, its
    /// Flick name.
    pub mappings: Vec<(u32, u32, Span, Option<String>)>,
    /// The Flick modules the code imports, types included.
    pub imports: Vec<String>,
}

#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
mod tests;
