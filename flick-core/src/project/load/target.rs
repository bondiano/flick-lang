//! Target runtime mode and reachable-use environment preflight.

#[cfg(not(target_arch = "wasm32"))]
use super::{Compilation, SourceMap};
use super::{Host, Project};
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::{Diagnostic, Severity};
#[cfg(not(target_arch = "wasm32"))]
use crate::project::environment::{ResolvedEnvironment, TargetOverride};
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
