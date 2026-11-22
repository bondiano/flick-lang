//! Backend-declared target restrictions: built-in modules and derived classes
//! that the selected backend does not support (`FLICK-P030`), and reachable
//! gated effects on a selected environment.

#[cfg(feature = "native-project")]
use super::Loader;
#[cfg(not(target_arch = "wasm32"))]
use super::{Compilation, SourceMap};
use super::{Host, Project};
#[cfg(any(feature = "native-project", not(target_arch = "wasm32")))]
use crate::diagnostics::Diagnostic;
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::Severity;
#[cfg(feature = "native-project")]
use crate::diagnostics::codes;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::target_preflight;

#[cfg(feature = "native-project")]
use crate::source::{FileId, Span};
#[cfg(feature = "native-project")]
use crate::syntax::cst::{Element, Node};
#[cfg(feature = "native-project")]
use crate::syntax::kind::SyntaxKind as K;

/// Whether `name`, or a module under it, matches an entry in `unsupported`.
#[cfg(feature = "native-project")]
pub(super) fn unsupported_match(unsupported: &[String], name: &str) -> bool {
    unsupported.iter().any(|m| {
        name.strip_prefix(m.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

/// P030 for a built-in module or derived class unsupported by `target`.
#[cfg(feature = "native-project")]
pub(super) fn unsupported_error(module: &str, target: &str, at: Span) -> Diagnostic {
    let message = format!("`{module}` is not supported by the `{target}` target");
    Diagnostic::error(codes::P030, message, at).with_fix(|| {
        (
            "remove the import, or build the project for a target that supports it".to_owned(),
            Vec::new(),
        )
    })
}

/// P030 for each derived class under `node` that appears in `unsupported`.
#[cfg(feature = "native-project")]
fn derived_unsupported(
    node: &Node,
    text: &str,
    file: FileId,
    unsupported: &[String],
    target: &str,
    found: &mut Vec<Diagnostic>,
) {
    for child in &node.children {
        match child {
            Element::Node(n) => derived_unsupported(n, text, file, unsupported, target, found),
            Element::Token(t) if node.kind == K::DerivingClause && t.kind == K::Con => {
                let class = t.text(text);
                if unsupported.iter().any(|u| u == class) {
                    found.push(unsupported_error(
                        class,
                        target,
                        Span::new(file, t.start, t.end),
                    ));
                }
            }
            Element::Token(_) => {}
        }
    }
}

#[cfg(feature = "native-project")]
impl Loader<'_> {
    /// P030 for each built-in import and derived class unsupported by the
    /// project's selected backend.
    pub(super) fn check_targets(&mut self) {
        let Some(project) = self.project else {
            return;
        };
        let unsupported = self.host.unsupported_modules(project);
        if unsupported.is_empty() {
            return;
        }
        let target = project.config.default_target();
        let mut found = Vec::new();
        for m in &self.modules {
            derived_unsupported(
                &m.parse.tree,
                &self.sources.get(m.file).text,
                m.file,
                &unsupported,
                target,
                &mut found,
            );
            for import in &m.syntax.imports {
                let name = import.module.as_str();
                if self.by_name.contains_key(name) {
                    continue;
                }
                let builtin = self.builtins().iter().any(|i| i.module == name);
                if builtin && unsupported_match(&unsupported, name) {
                    found.push(unsupported_error(name, target, import.span));
                }
            }
        }
        self.diagnostics.extend(found);
    }
}

/// Check the selected environment against the complete compiled program.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn check_environment(
    host: &dyn Host,
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
        target_preflight::check(
            compilation,
            &entries,
            &runners,
            sources,
            environment,
            &|service| {
                project.is_none_or(|p| host.handles_service(p, environment, service) != Some(false))
            },
            diagnostics,
        );
    }
}

/// Whether a run over `project` checks with the stages of a class derived
/// from it: explicit `Some(true)`/`Some(false)` overrides the project, while
/// `None` queries the host's backend descriptor for `project`.
pub fn stages_by_class(host: &dyn Host, project: Option<&Project>, rt: Option<bool>) -> bool {
    match rt {
        Some(explicit) => explicit,
        #[cfg(feature = "native-project")]
        None => project.is_some_and(|p| host.stages_by_class(p)),
        #[cfg(not(feature = "native-project"))]
        None => {
            let _ = host;
            project.is_some()
        }
    }
}
