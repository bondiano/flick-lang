//! The error for an import of a module nothing provides.

use std::path::Path;

use super::Loader;
use crate::diagnostics::suggest::closest;
use crate::diagnostics::{Diagnostic, codes};
use crate::project::module_path;
use crate::source::Span;
use crate::types;

impl Loader<'_> {
    pub(super) fn unknown_module(&self, name: &str, span: Span) -> Diagnostic {
        let missing = self.packages.missing();
        if missing.is_empty() {
            let local = module_path(name);
            // shortcut: local modules nothing has loaded yet are not candidates
            let known = self
                .by_name
                .keys()
                .chain(self.from_packages.keys())
                .map(String::as_str);
            let builtin = types::builtins().iter().map(|i| i.module.as_str());
            let guess = closest(name, known.chain(builtin)).map(str::to_owned);
            return Diagnostic::error(codes::R003, format!("unknown module `{name}`"), span)
                .did_you_mean(span, guess)
                .with_fix(
                format!(
                    "a local module goes in `{}`; if it comes from an npm package, add it with `flick add <package>`",
                    self.project.map_or(local.clone(), |p| Path::new(&p.config.source_dir).join(&local)).display()
                ),
                Vec::new(),
            );
        }
        let list = missing
            .iter()
            .map(|m| format!("`{m}`"))
            .collect::<Vec<_>>()
            .join(", ");
        Diagnostic::error(
            codes::P009,
            format!("cannot find module `{name}`; dependencies in package.json are not installed: {list}"),
            span,
        )
        .with_fix("install them with `flick install`", Vec::new())
    }
}
