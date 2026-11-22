//! Target runtime mode and reachable-use environment preflight.

use super::Project;
#[cfg(not(target_arch = "wasm32"))]
use super::{Compilation, SourceMap};
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
    target_preflight::check(
        compilation,
        &CheckOptions {
            entries: &entries,
            runners: &runners,
            by_class,
            for_tests,
            test_paths,
        },
        sources,
        environment,
        diagnostics,
    );
}

/// Whether a run over `project` checks with the stages of a class derived
/// from it, as Flick's own runtime has them: explicit `Some(true)`/`Some(false)`
/// overrides the project, while `None` follows the project's `[target.js]`
/// runtime (`"rt"` by default). A host prepares the built-in modules checked
/// this way before it runs.
pub fn stages_by_class(project: Option<&Project>, rt: Option<bool>) -> bool {
    match rt {
        Some(explicit) => explicit,
        #[cfg(feature = "native-project")]
        None => project.is_some_and(|p| !p.config.is_go() && p.config.js_is_rt()),
        #[cfg(not(feature = "native-project"))]
        None => project.is_some(),
    }
}
