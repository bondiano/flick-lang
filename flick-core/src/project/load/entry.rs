//! The entry module of an app.

use std::path::Path;

use super::{Loader, Root};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::project::config::Kind;
use crate::project::{is_module_name, module_path};
use crate::resolve::Interface;

impl Loader<'_> {
    /// The entry of an app is a source module that exports `main`.
    pub(super) fn check_entry(&mut self, interfaces: &[Option<Interface>]) {
        let Some(project) = self.project.filter(|p| p.config.kind == Kind::App) else {
            return;
        };
        let Some((entry, span)) = project.config.entry.clone() else {
            return;
        };
        let error = |message: String| Diagnostic::error(codes::P011, message, span);
        if !is_module_name(&entry) {
            self.diagnostics
                .push(error(format!("`{entry}` is not a module name")));
            return;
        }
        let expected = project.source_root().join(module_path(&entry));
        let shown = Path::new(&project.config.source_dir).join(module_path(&entry));
        match self.by_name.get(&entry).copied() {
            Some(i) if self.modules[i].root == Root::Source => {
                if !interfaces[i]
                    .as_ref()
                    .is_some_and(|i| i.exports_value("main"))
                {
                    self.diagnostics.push(
                        error(format!("entry module `{entry}` does not export `main`")).with_fix(
                            format!("add `main` to `module {entry} exposing (main)`"),
                            Vec::new(),
                        ),
                    );
                }
            }
            Some(_) => self.diagnostics.push(error(format!(
                "entry module `{entry}` is not in the source root: expected `{}`",
                shown.display()
            ))),
            // exists, but not among the modules this check covers
            None if self.host.read(&expected).is_ok() => {}
            None => self.diagnostics.push(error(format!(
                "entry module `{entry}` not found: expected `{}`",
                shown.display()
            ))),
        }
    }
}
