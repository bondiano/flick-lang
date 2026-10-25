//! Installed-interface graph closure and package compatibility checks.

use std::collections::BTreeSet;

use super::Loader;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types;

impl Loader<'_> {
    pub(super) fn check_packages(&mut self) {
        // Package runners may add interfaces while entry points are checked.
        for (module, package) in &self.owners {
            if let Some(&local) = self.by_name.get(module) {
                let local = &self.modules[local];
                self.diagnostics.push(Diagnostic::error(
                    codes::R002,
                    format!("module `{module}` is also provided by package `{package}`"),
                    local
                        .syntax
                        .header
                        .as_ref()
                        .map_or(Span::new(local.file, 0, 0), |h| h.span),
                ));
            }
        }
        self.diagnostics.extend(super::super::packages::stale(
            &self.from_packages,
            &self.owners,
        ));
    }

    /// Close both dependency fingerprints and re-export origins before emission.
    pub(super) fn load_package_interfaces(&mut self, name: &str, span: Span) {
        let mut pending = BTreeSet::from([name.to_owned()]);
        while let Some(module) = pending.pop_first() {
            if self.from_packages.contains_key(&module) {
                continue;
            }
            let mut found = self
                .packages
                .find(self.host, &module, &mut self.diagnostics);
            let Some((package, interface)) = found.pop() else {
                // The caller handles an absent initial module, which may be local.
                if module != name {
                    match types::builtins().iter().find(|i| i.module == module) {
                        Some(builtin) => {
                            self.from_packages.insert(module, builtin.clone());
                        }
                        None => self.diagnostics.push(self.unknown_module(&module, span)),
                    }
                }
                continue;
            };
            if let Some((other, _)) = found.first() {
                self.diagnostics.push(Diagnostic::error(
                    codes::R002,
                    format!("module `{module}` is provided by both `{other}` and `{package}`"),
                    span,
                ));
            }
            pending.extend(interface.dependencies.keys().cloned());
            pending.extend(
                interface
                    .exports
                    .iter()
                    .map(|e| &e.entity.module)
                    .filter(|origin| origin.as_str() != module)
                    .cloned(),
            );
            self.owners.insert(module.clone(), package);
            self.from_packages.insert(module, interface);
        }
    }
}
