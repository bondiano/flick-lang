//! Target runtime mode and reachable-use environment preflight.

use super::{Compilation, Host, Mode, Project, SourceMap};
use crate::diagnostics::{Diagnostic, Severity};
#[cfg(feature = "native-project")]
use crate::ir;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::environment::ResolvedEnvironment;
use crate::project::environment::TargetOverride;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::target_preflight::{self, CheckOptions, RunnerRef};
#[cfg(not(target_arch = "wasm32"))]
use crate::source::Span;

/// Check the resolved environment against the complete compiled program.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub(super) fn check_environment(
    host: &dyn Host,
    compilation: &Compilation,
    project: Option<&Project>,
    target_override: Option<&TargetOverride>,
    by_class: bool,
    environment: &ResolvedEnvironment,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if compilation.modules.is_empty() || diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return;
    }
    let entries: Vec<(String, String, Option<Span>)> = project
        .map(|p| {
            p.config
                .entries
                .iter()
                .map(|e| (e.name.clone(), e.module.clone(), Some(e.span)))
                .collect()
        })
        .unwrap_or_default();
    let runners: Vec<RunnerRef> = project
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
        .collect();
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
}

/// Runs environment preflight, optional backend `check-target` validation,
/// official adapter fallbacks, and deterministic diagnostic sorting.
#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_diagnostics(
    host: &dyn Host,
    mode: Mode,
    compilation: &Compilation,
    project: Option<&Project>,
    target_override: Option<&TargetOverride>,
    stages_by_class: bool,
    sources: &mut SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
    adapter_diagnostics: Vec<Diagnostic>,
) {
    #[cfg(target_arch = "wasm32")]
    let _ = (compilation, project, target_override, stages_by_class);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(resolved) = compilation.environment.as_ref() {
        check_environment(
            host,
            compilation,
            project,
            target_override,
            stages_by_class,
            resolved,
            sources,
            diagnostics,
        );
    }
    let has_error = diagnostics.iter().any(|d| d.severity == Severity::Error);
    if mode == Mode::Compile {
        if has_error || !host.has_backend_runner() {
            diagnostics.extend(adapter_diagnostics);
        }
    } else {
        #[cfg(feature = "native-project")]
        let backend_checked = if !has_error
            && host.has_backend_runner()
            && let Some(project) = project
            && let Some(resolved) = compilation.environment.as_ref()
            && compilation.modules.iter().any(|m| {
                m.package.is_none()
                    && m.ir
                        .items
                        .iter()
                        .any(|i| matches!(i.kind, ir::ItemKind::Foreign(_)))
            }) {
            host.check_target(project, compilation, resolved, stages_by_class, sources)
        } else {
            None
        };
        #[cfg(not(feature = "native-project"))]
        let backend_checked: Option<Vec<Diagnostic>> = None;
        match backend_checked {
            Some(backend_diags) => diagnostics.extend(backend_diags),
            None => diagnostics.extend(adapter_diagnostics),
        }
    }
    // by path, not by load order, which follows the order of imports
    diagnostics.sort_by_cached_key(|d| {
        (
            d.span.is_none(),
            d.span
                .map(|s| (sources.get(s.file).path.clone(), s.start, s.end)),
        )
    });
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
