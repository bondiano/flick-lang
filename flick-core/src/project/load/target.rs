//! Target runtime mode, reachable-use environment preflight, and in-process
//! official-backend `@export` validation.

#[cfg(not(target_arch = "wasm32"))]
use super::{Compilation, SourceMap};
use super::{Host, Project};
#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "native-js", feature = "native-project")
))]
use crate::diagnostics::codes;
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::{Diagnostic, Severity};
#[cfg(not(target_arch = "wasm32"))]
use crate::ir::Module as IrModule;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::environment::{ResolvedEnvironment, TargetOverride};
#[cfg(not(target_arch = "wasm32"))]
use crate::project::target_preflight::{self, CheckOptions, RunnerRef};
#[cfg(not(target_arch = "wasm32"))]
use crate::source::Span;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-project"))]
use flick_backend_go::exports as go_exports;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-js"))]
use flick_backend_js::exports as js_exports;

/// Check the resolved environment against the complete compiled program.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub(super) fn check_environment(
    host: &dyn Host,
    compilation: &Compilation,
    project: Option<&Project>,
    target_override: Option<&TargetOverride>,
    by_class: bool,
    library_target: bool,
    environment: &ResolvedEnvironment,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if compilation.modules.is_empty() || diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return;
    }
    let entries: Vec<(String, String, Option<Span>)> = if library_target {
        Vec::new()
    } else {
        project
            .map(|p| {
                p.config
                    .entries
                    .iter()
                    .map(|e| (e.name.clone(), e.module.clone(), Some(e.span)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let runners: Vec<RunnerRef> = if library_target {
        Vec::new()
    } else {
        project
            .iter()
            .flat_map(|p| &p.config.entries)
            .filter_map(|e| {
                if let Some((runner, span)) = &e.runner {
                    Some(RunnerRef {
                        entry: e.name.clone(),
                        module: runner.clone(),
                        span: Some(*span),
                    })
                } else if e.client {
                    Some(RunnerRef {
                        entry: e.name.clone(),
                        module: "Runner.Browser".to_owned(),
                        span: Some(e.span),
                    })
                } else {
                    None
                }
            })
            .collect()
    };
    let for_tests = target_override.is_some_and(|o| o.for_tests);
    let test_paths = target_override.map_or(&[][..], |o| o.test_paths.as_slice());
    let overridden = target_override.is_some_and(|o| o.target_overridden);
    let pair = environment.pair();
    let handles = |service: &str| {
        if overridden {
            None
        } else {
            project.and_then(|p| host.handles_service(p, &pair, service))
        }
    };
    target_preflight::check(
        compilation,
        &CheckOptions {
            entries: &entries,
            runners: &runners,
            by_class,
            for_tests,
            test_paths,
            handles: Some(&handles),
        },
        sources,
        environment,
        diagnostics,
    );
    if !target_override.is_some_and(|o| o.skip_backend_check_target)
        && !diagnostics.iter().any(|d| d.severity == Severity::Error)
    {
        validate_official_exports(compilation, &environment.backend_id, diagnostics);
        if !diagnostics.iter().any(|d| d.severity == Severity::Error)
            && let Some(project) = project
        {
            diagnostics.extend(host.check_target(
                project,
                compilation,
                sources,
                environment,
                by_class,
            ));
        }
    }
}

/// Validates `@export "name"` attributes across own active modules in-process
/// for the official JS and Go backends so `flick check` and LSP report invalid,
/// duplicate, or non-exportable host exports at their attribute spans without
/// starting a WASM backend component.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn validate_official_exports(
    compilation: &Compilation,
    backend_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let own: Vec<&IrModule> = compilation
        .modules
        .iter()
        .filter(|m| m.package.is_none() && compilation.module_available(&m.name))
        .map(|m| &*m.ir)
        .collect();
    if own.iter().all(|m| m.exports.host.is_empty()) {
        return;
    }
    match backend_id {
        #[cfg(feature = "native-js")]
        "flick/js" => {
            for diag in js_exports::validate_modules(own) {
                let mut msg = diag.message;
                for note in &diag.notes {
                    msg.push_str("\nnote: ");
                    msg.push_str(note);
                }
                diagnostics.push(Diagnostic::error(codes::P012, msg, diag.span));
            }
        }
        #[cfg(feature = "native-project")]
        "flick/go" => {
            for diag in go_exports::validate_modules(own) {
                let mut msg = diag.message;
                for note in &diag.notes {
                    msg.push_str("\nnote: ");
                    msg.push_str(note);
                }
                diagnostics.push(Diagnostic::error(codes::P012, msg, diag.span));
            }
        }
        _ => {
            let _ = (own, diagnostics);
        }
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

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn has_own_export_attrs(modules: &[super::Loaded], sources: &SourceMap) -> bool {
    use crate::syntax::{cst::Node, export_attr, kind::SyntaxKind as K};
    modules.iter().any(|m| {
        if !matches!(m.root, super::Root::Source | super::Root::Other(_)) {
            return false;
        }
        let text = &sources.get(m.file).text;
        m.parse.tree.nodes().any(|n| match n.kind {
            K::Attribute => export_attr::is_export(n, text),
            K::RecGroup => n
                .nodes()
                .filter(|c| c.kind == K::Members)
                .flat_map(Node::nodes)
                .any(|c| c.kind == K::Attribute && export_attr::is_export(c, text)),
            _ => false,
        })
    })
}
