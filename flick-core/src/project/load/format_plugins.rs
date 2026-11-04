//! The plugins the formatter lays out blocks with: the plugin of the package
//! each imported module belongs to, found from the source directories of
//! the project's packages, without checking any module.

use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "native-project")]
use super::super::packages::Packages;
use super::super::{Host, Project};
#[cfg(feature = "native-project")]
use super::module_path;
use crate::plugin::{Expander, first_party};
use crate::resolve;
use crate::source::{FileId, SourceMap};
use crate::syntax;
use crate::types;

/// The plugins of a project's packages, each loaded once.
pub struct FormatPlugins<'h> {
    host: &'h dyn Host,
    #[cfg(feature = "native-project")]
    packages: Packages,
    /// Each package asked for and its plugin, `None` when it has none or
    /// does not load: its blocks stay as written.
    loaded: HashMap<String, Option<Arc<dyn Expander>>>,
}

impl<'h> FormatPlugins<'h> {
    /// The plugins of the packages of `project`; without one, only `std`'s.
    pub fn new(host: &'h dyn Host, sources: &mut SourceMap, project: Option<&Project>) -> Self {
        #[cfg(feature = "native-project")]
        let packages = project.map_or_else(Packages::default, |p| {
            // what is wrong with them is for `flick check` to say
            Packages::load(host, sources, p, &mut Vec::new())
        });
        #[cfg(not(feature = "native-project"))]
        let _ = (sources, project);
        FormatPlugins {
            host,
            #[cfg(feature = "native-project")]
            packages,
            loaded: HashMap::new(),
        }
    }

    /// The plugin of each head the imports of `text` expose.
    pub fn of(&mut self, text: &str, file: FileId) -> HashMap<String, Arc<dyn Expander>> {
        let parsed = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parsed.tree, text, file);
        let mut out = HashMap::new();
        for import in syntax.imports.iter().filter(|i| !i.plugins.is_empty()) {
            let Some(package) = self.package_of(&import.module) else {
                continue;
            };
            let plugin = self.plugin(&package);
            for item in &import.plugins {
                if let Some(plugin) = &plugin {
                    out.insert(item.name.clone(), Arc::clone(plugin));
                }
            }
        }
        out
    }

    /// The package `module` belongs to: `std` for its modules, else the
    /// package whose source directory has its file.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    fn package_of(&self, module: &str) -> Option<String> {
        if types::is_std_module(module) {
            return Some("std".to_owned());
        }
        #[cfg(feature = "native-project")]
        {
            let file = module_path(module);
            (self.packages.sources())
                .find(|(_, dir)| self.host.read(&dir.join(&file)).is_ok())
                .map(|(name, _)| name.to_owned())
        }
        #[cfg(not(feature = "native-project"))]
        None
    }

    fn plugin(&mut self, package: &str) -> Option<Arc<dyn Expander>> {
        if let Some(plugin) = self.loaded.get(package) {
            return plugin.clone();
        }
        #[cfg(feature = "native-project")]
        let component = self.packages.plugin(package);
        #[cfg(not(feature = "native-project"))]
        let component = None;
        let plugin =
            first_party::plugin(package).or_else(|| self.host.plugin(package, component).ok());
        self.loaded.insert(package.to_owned(), plugin.clone());
        plugin
    }
}
