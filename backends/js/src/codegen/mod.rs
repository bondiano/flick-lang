//! JavaScript target emission from portable IR and plain frontend projections.

mod js;
mod lexical;
pub mod runner;
#[cfg(feature = "source-maps")]
pub mod sourcemap;
pub mod ts;

pub use flick_backend_support::{EffectBridge, Known, exports, loops, names, order, tail};
pub use js::{direct_methods, exported_worker, js, relative};
pub use lexical::{RuntimeLayout, js_parts, prefix_key};
pub use names::{ident, instance as instance_export, qualified_id, tag};

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::effects::runtime_specifier;
#[cfg(feature = "source-maps")]
use flick_ir::Span;

/// The directory of the output under which the built-in modules are.
pub const BUILTIN_DIR: &str = "_flick";

/// The import specifier the build gives the foreign module `spec` named in
/// `module`.
pub struct Resolved {
    pub module: String,
    pub spec: String,
    pub specifier: String,
}

/// Where modules are and who owns them, as the build lays them out.
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// The built-in modules, written under [`BUILTIN_DIR`].
    pub builtins: &'a [&'a str],
    /// Explicit physical output paths supplied by a component request.
    /// The native development adapter uses its established default layout.
    pub paths: Option<&'a BTreeMap<String, String>>,
    /// The package of each module that belongs to one, for its constructor
    /// ids.
    pub packages: &'a BTreeMap<String, String>,
    /// The import specifier the build gives a foreign module named in a
    /// module, by the two: one named relative to its module, shipped beside
    /// it, or the renderer `[build] ui` stands for. Another is a package's,
    /// which the runtime decides (see [`Options::foreign`]).
    pub resolved: &'a [Resolved],
    /// Emit TypeScript (`--emit ts`) instead of JavaScript: with the types
    /// of the exports of the module to compile, which the build prepares
    /// for each physical module of it as it writes the module.
    pub typescript: Option<&'a BTreeMap<String, ts::Typed>>,
    /// The release profile: worker functions for known full applications
    /// and direct methods of canonical instances.
    pub release: bool,
    /// In release, the arity `n` of the worker `name$n` that a module,
    /// this one included, exports for its value `name` (see
    /// [`exported_worker`]); a full application calls it.
    pub workers: &'a HashMap<(String, String), usize>,
    /// In release, the methods each module exports as a constant of its
    /// instance (see [`direct_methods`]): by module, instance and method.
    pub direct: &'a HashSet<(String, String, String)>,
    /// In release, which top-level values keep their label; debug labels
    /// every atom, view and action.
    pub labels: Labels,
    /// Compile for Flick's own runtime instead of Effect (see `js::kernel`).
    pub rt: bool,
    /// Whether RT code may import the optional Effect adapter.
    pub effect_bridge: EffectBridge,
    /// The program's lang items, the built-in entities the code knows.
    pub lang: &'a Known<'a>,
}

impl Options<'_> {
    pub(crate) fn typescript(&self) -> bool {
        self.typescript.is_some()
    }

    /// The output path of a module, relative to the output directory:
    /// `App/Users.js`.
    pub fn path(&self, module: &str) -> String {
        self.paths
            .and_then(|paths| paths.get(module))
            .cloned()
            .unwrap_or_else(|| module_path(self.builtins, self.typescript(), module))
    }

    /// The import specifier of a foreign module `spec` named in `module`.
    pub fn foreign(&self, module: &str, spec: &str) -> String {
        let found = self
            .resolved
            .iter()
            .find(|r| r.module == module && r.spec == spec);
        found.map_or_else(|| runtime_specifier(self.rt, spec), |r| r.specifier.clone())
    }

    /// The package `module` belongs to, if any.
    pub(crate) fn package(&self, module: &str) -> Option<&str> {
        self.packages.get(module).map(String::as_str)
    }

    /// In release, the arity of the worker `module` exports for `name`.
    pub(crate) fn worker(&self, module: &str, name: &str) -> Option<usize> {
        let key = (module.to_owned(), name.to_owned());
        self.workers.get(&key).copied()
    }

    /// In release, whether `module` exports `method` of `instance` as a
    /// constant.
    pub(crate) fn is_direct(&self, module: &str, instance: &str, method: &str) -> bool {
        let key = (module.to_owned(), instance.to_owned(), method.to_owned());
        self.direct.contains(&key)
    }
}

/// The output path of `module`, relative to the output directory, in
/// TypeScript or JavaScript: the built-in modules are under [`BUILTIN_DIR`].
pub fn module_path(builtins: &[&str], typescript: bool, module: &str) -> String {
    let ext = if typescript { "ts" } else { "js" };
    let file = format!("{}.{ext}", module.replace('.', "/"));
    if builtins.contains(&module) {
        format!("{BUILTIN_DIR}/{file}")
    } else {
        file
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
    #[cfg(feature = "source-maps")]
    /// Each generated position, 0-based line and UTF-16 column, with the
    /// source node it comes from and, at the name of a top-level value, its
    /// Flick name.
    pub mappings: Vec<(u32, u32, Span, Option<String>)>,
    /// The Flick modules the code imports, types included.
    pub imports: Vec<String>,
}
