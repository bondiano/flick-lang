//! What a program built for go cannot use: built-in modules that exist on
//! the js target only, `deriving Form`, and package modules no build
//! compiled for go. Selected environments also check reachable effects.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
use super::{Compilation, SourceMap};
#[cfg(not(target_arch = "wasm32"))]
use crate::build::{Extras, Settings, target_preflight};
#[cfg(not(target_arch = "wasm32"))]
use crate::codegen::EffectBridge;
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::Severity;

use super::{Loader, Project};
use crate::diagnostics::{Diagnostic, codes};

use crate::project::config::{Backend, Emit};
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

/// Check the selected environment against the complete compiled program.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn check_environment(
    compilation: &Compilation,
    project: Option<&Project>,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(environment) = project.and_then(|p| p.config.build.environment.as_ref())
        && !compilation.modules.is_empty()
        && !diagnostics.iter().any(|d| d.severity == Severity::Error)
    {
        let entries: Vec<(String, String)> = project
            .map(|p| {
                p.config
                    .entries
                    .iter()
                    .map(|e| (e.name.clone(), e.module.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let runners: Vec<(String, String)> = project
            .iter()
            .flat_map(|p| &p.config.entries)
            .filter_map(|e| Some((e.name.clone(), e.runner.as_ref()?.0.clone())))
            .collect();
        let settings = Settings {
            package: None,
            entries: &entries,
            typescript: false,
            release: false,
            extras: Extras::default(),
            source_path: &|_| String::new(),
            source_dir: &|_| None,
            out_dir: Path::new(""),
            root: project.map(|p| p.root.as_path()),
            backend: project.map_or(Backend::Effect, |p| p.config.build.effect),
            effect_bridge: EffectBridge::Absent,
            ui: project.and_then(|p| p.config.build.ui.as_deref()),
            runners: &runners,
            environment: Some(environment),
        };
        target_preflight::check(compilation, &settings, sources, environment, diagnostics);
    }
}

/// Whether a run over `project` checks with the stages of a class derived
/// from it, as Flick's own runtime has them: asked for with `rt`, or the
/// project's runtime, whatever the target of the build. A host prepares the
/// built-in modules checked this way before it runs.
pub fn stages_by_class(project: Option<&Project>, rt: bool) -> bool {
    rt || project.is_some_and(|p| p.config.build.effect == Backend::Rt)
}
