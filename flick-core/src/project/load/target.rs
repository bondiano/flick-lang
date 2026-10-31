//! What a program built for go cannot use: built-in modules that exist on
//! the js target only, and package modules no build compiled for go.

use super::Loader;
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::Target;
use crate::project::config::Emit;
use crate::source::Span;
use crate::types;

/// The built-in modules of the js target only: the browser UI and its
/// reactivity, the RPC and telemetry clients, JS interop and the JS hosts.
const JS_ONLY: &[&str] = &[
    "Action",
    "Atom",
    "DevTools",
    "Js",
    "Otlp",
    "Reactivity.Atom",
    "Rpc",
    "Runner.Browser",
    "Runner.Cloudflare",
    "Runner.Node",
    "Ui",
];

/// Whether the built-in module `name`, or a module under it, is js only.
pub(crate) fn js_only(name: &str) -> bool {
    JS_ONLY.iter().any(|m| {
        name.strip_prefix(m)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

/// P030 for a module of the js target only, named at `at`.
pub(super) fn js_only_error(module: &str, at: Span) -> Diagnostic {
    let message =
        format!("`{module}` is available on the js target only, and this project emits go");
    Diagnostic::error(codes::P030, message, at).with_fix(|| {
        (
            "remove the import, or build the project with `emit = \"js\"`".to_owned(),
            Vec::new(),
        )
    })
}

impl Loader<'_> {
    /// P030 for each import of a program built for go that the go target
    /// does not have.
    pub(super) fn check_targets(&mut self) {
        if self.project.is_none_or(|p| p.config.build.emit != Emit::Go) {
            return;
        }
        let mut found = Vec::new();
        for m in &self.modules {
            for import in &m.syntax.imports {
                let name = import.module.as_str();
                if self.by_name.contains_key(name) {
                    continue;
                }
                let builtin = types::builtins().iter().any(|i| i.module == name);
                if builtin && js_only(name) {
                    found.push(js_only_error(name, import.span));
                } else if let Some(i) = self.from_packages.get(name)
                    && !builtin
                    && !i.targets.contains(&Target::Go)
                {
                    let message = format!(
                        "the package module `{name}` is built for the js target only, and this project emits go"
                    );
                    found.push(Diagnostic::error(codes::P030, message, import.span));
                }
            }
        }
        self.diagnostics.extend(found);
    }
}
