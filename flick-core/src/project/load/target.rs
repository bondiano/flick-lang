//! What a program built for go cannot use: built-in modules that exist on
//! the js target only, `deriving Form`, and package modules no build
//! compiled for go.

use super::Loader;
use crate::diagnostics::{Diagnostic, codes};

use crate::project::config::Emit;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

/// The built-in modules of the js target only: the browser UI, its forms
/// and its reactivity, the telemetry client, JS interop and the
/// JS hosts.
const JS_ONLY: &[&str] = &[
    "Action",
    "Atom",
    "DevTools",
    "Form",
    "Js",
    "Otlp",
    "Reactivity.AsyncResult",
    "Reactivity.Atom",
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

/// P030 for each `Form` of a `deriving` clause under `node`: a form is
/// built on `Form`, though the module need not import it.
fn derived_forms(node: &Node, text: &str, file: FileId, found: &mut Vec<Diagnostic>) {
    for child in &node.children {
        match child {
            Element::Node(n) => derived_forms(n, text, file, found),
            Element::Token(t)
                if node.kind == K::DerivingClause && t.kind == K::Con && t.text(text) == "Form" =>
            {
                found.push(js_only_error("Form", Span::new(file, t.start, t.end)));
            }
            Element::Token(_) => {}
        }
    }
}

impl Loader<'_> {
    /// P030 for each import of a program built for go that the go target
    /// does not have, and each `deriving Form`.
    pub(super) fn check_targets(&mut self) {
        if self.project.is_none_or(|p| p.config.build.emit != Emit::Go) {
            return;
        }
        let mut found = Vec::new();
        for m in &self.modules {
            derived_forms(
                &m.parse.tree,
                &self.sources.get(m.file).text,
                m.file,
                &mut found,
            );
            for import in &m.syntax.imports {
                let name = import.module.as_str();
                if self.by_name.contains_key(name) {
                    continue;
                }
                let builtin = self.builtins().iter().any(|i| i.module == name);
                if builtin && js_only(name) {
                    found.push(js_only_error(name, import.span));
                }
            }
        }
        self.diagnostics.extend(found);
    }
}
