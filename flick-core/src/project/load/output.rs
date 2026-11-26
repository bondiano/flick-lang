//! Compiled output structures (`Compilation`, `Compiled`) produced by project
//! loading.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{Loaded, Lowered, Root};
use crate::ir;
use crate::plugin::expansion::Expansion;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::environment::ResolvedEnvironment;
#[cfg(feature = "native-project")]
use crate::project::target;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::target_preflight;
use crate::project::{companion_of, normalize};
use crate::resolve::Interface;
use crate::source::FileId;
#[cfg(not(target_arch = "wasm32"))]
use crate::source::SourceMap;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::NativeOverrideEnv;

/// The module `m` of the build, lowered.
pub(super) fn compiled(
    m: &Loaded,
    (ir, declarations, _): Lowered,
    interface: Interface,
) -> Compiled {
    Compiled {
        name: m.name.clone(),
        path: m.path.clone(),
        file: m.file,
        test: m.root == Root::Test || companion_of(&m.name).is_some(),
        package: match &m.root {
            Root::Package(name) => Some(name.clone()),
            _ => None,
        },
        imports: m
            .syntax
            .imports
            .iter()
            .chain(&m.syntax.inline_imports)
            .filter(|i| !i.module.starts_with("$scope:"))
            .map(|i| i.module.clone())
            .chain(
                m.virtual_of
                    .iter()
                    .flat_map(|v| v.capture.scopes.iter().map(|s| s.module.clone())),
            )
            .collect(),
        ir,
        declarations,
        interface,
        expansion: m.expansion.clone(),
    }
}

/// The modules of a build: the project's and those of its libraries.
#[derive(Default)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone))]
pub struct Compilation {
    pub modules: Vec<Compiled>,
    /// The libraries of `[dependencies]`, each with its source directory.
    pub libraries: Vec<(String, PathBuf)>,
    /// What the closure asks of backends' ecosystems.
    #[cfg(feature = "native-project")]
    pub target_dependencies: Vec<target::Wanted>,
    /// Dependency packages that declare `[package] when = "<cond>"`, by package name.
    #[cfg(not(target_arch = "wasm32"))]
    pub package_when: BTreeMap<String, String>,
    /// Own project's `(package name, when condition)` when `[package] when` is set.
    #[cfg(not(target_arch = "wasm32"))]
    pub own_package_when: Option<(String, String)>,
    /// The environment resolved for this compilation, when a target/environment
    /// was selected and resolved cleanly.
    #[cfg(not(target_arch = "wasm32"))]
    pub environment: Option<ResolvedEnvironment>,
    /// The environment the `@native` clauses were selected in, by which a
    /// build selects those of the built-in modules too; `None` for the
    /// default one, which their shared IR lowers.
    #[cfg(not(target_arch = "wasm32"))]
    pub native_overrides: Option<NativeOverrideEnv>,
}

/// A module that checked cleanly, lowered to the IR.
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone))]
pub struct Compiled {
    pub name: String,
    pub path: PathBuf,
    pub file: FileId,
    /// Whether it is in the test root.
    pub test: bool,
    /// The source package it belongs to; `None` for the project's own.
    pub package: Option<String>,
    /// The modules it imports.
    pub imports: Vec<String>,
    pub ir: Arc<ir::Module>,
    /// Every top-level entity with its type, for the reference model.
    pub declarations: Arc<Interface>,
    pub interface: Interface,
    /// The module with its plugin blocks expanded, when it has any.
    pub expansion: Option<Arc<Expansion>>,
}

impl Compilation {
    /// Whether `module` (of `self.modules`) is available in `self.environment`
    /// (both its own `@when` and its package's `[package] when` hold, or no
    /// environment was resolved).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn module_available(&self, module: &str) -> bool {
        let Some(env) = &self.environment else {
            return true;
        };
        let Some(compiled) = self.modules.iter().find(|m| m.name == module) else {
            return true;
        };
        if let Some(cond) = &compiled.interface.when
            && !env.matches(cond)
        {
            return false;
        }
        let pkg_cond = match &compiled.package {
            Some(pkg) => self.package_when.get(pkg).map(String::as_str),
            None => self.own_package_when.as_ref().map(|(_, c)| c.as_str()),
        };
        pkg_cond.is_none_or(|cond| env.matches(cond))
    }

    /// Returns a copy of `self` with gated-out modules and dead items that
    /// reference unavailable modules/facades or missing Effect-adapter bindings
    /// removed, so backends never emit or statically import unavailable code.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn prune_for_environment(&self, sources: &SourceMap, by_class: bool) -> Self {
        target_preflight::prune_compilation(self, sources, by_class)
    }

    /// The library of `[dependencies]` that `module` belongs to, with its
    /// source directory.
    pub fn library_of(&self, module: &str) -> Option<(&str, &Path)> {
        let module = self.modules.iter().find(|m| m.name == module)?;
        let package = module.package.as_deref()?;
        (self.libraries.iter())
            .find(|(name, _)| name == package)
            .map(|(name, dir)| (name.as_str(), dir.as_path()))
    }

    /// The file of the foreign module `spec` when the build ships it beside
    /// `module`: a relative non-`.fl` file of a library inside its source
    /// directory. Any other relative file stays where it is, and the output
    /// refers to it.
    pub fn shipped_foreign(&self, module: &str, spec: &str) -> Option<PathBuf> {
        if (!spec.starts_with("./") && !spec.starts_with("../"))
            || Path::new(spec)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("fl"))
        {
            return None;
        }
        let (_, dir) = self.library_of(module)?;
        let source = self.modules.iter().find(|m| m.name == module)?;
        let path = normalize(&source.path.parent()?.join(spec));
        path.starts_with(dir).then_some(path)
    }
}
