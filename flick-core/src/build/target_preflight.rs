//! Build-facing compatibility API for unified project environment preflight.

use std::collections::BTreeSet;

use super::{Backend, Settings};
use crate::diagnostics::Diagnostic;
use crate::project::Compilation;
use crate::project::config::BackendEnvironment;
use crate::project::environment::ResolvedEnvironment;
use crate::project::target_preflight::{self, CheckOptions, RunnerRef};
use crate::source::SourceMap;

pub use target_preflight::{gated_service, handled_services, handles, std_root_handlers};

/// Run unified environment preflight from native JS build `Settings` (used by
/// direct `build::emit` callers and the JS backend preflight step).
pub fn check_settings(
    compilation: &Compilation,
    settings: &Settings<'_>,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let env = resolved_from_settings(compilation, settings);
    let entries: Vec<_> = settings
        .entries
        .iter()
        .map(|(name, module)| (name.clone(), module.clone(), None))
        .collect();
    let runners: Vec<_> = settings
        .runners
        .iter()
        .map(|(entry, module)| RunnerRef {
            entry: entry.clone(),
            module: module.clone(),
            span: None,
        })
        .collect();
    target_preflight::check(
        compilation,
        &CheckOptions {
            entries: &entries,
            runners: &runners,
            by_class: settings.backend == Backend::Rt,
            for_tests: false,
            test_paths: &[],
        },
        sources,
        &env,
        diagnostics,
    );
}

/// Reject reachable unavailable uses and gated effects for an explicit environment.
pub fn check(
    compilation: &Compilation,
    settings: &Settings<'_>,
    sources: &SourceMap,
    environment: &BackendEnvironment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut env = resolved_from_settings(compilation, settings);
    env.backend.clone_from(&environment.backend);
    env.platform.clone_from(&environment.platform);
    env.tags.insert(environment.backend.clone());
    env.tags.insert(environment.platform.clone());
    env.explicit_environment = true;
    let entries: Vec<_> = settings
        .entries
        .iter()
        .map(|(name, module)| (name.clone(), module.clone(), None))
        .collect();
    let runners: Vec<_> = settings
        .runners
        .iter()
        .map(|(entry, module)| RunnerRef {
            entry: entry.clone(),
            module: module.clone(),
            span: None,
        })
        .collect();
    target_preflight::check(
        compilation,
        &CheckOptions {
            entries: &entries,
            runners: &runners,
            by_class: settings.backend == Backend::Rt,
            for_tests: false,
            test_paths: &[],
        },
        sources,
        &env,
        diagnostics,
    );
}

pub(crate) fn resolved_from_settings(
    compilation: &Compilation,
    settings: &Settings<'_>,
) -> ResolvedEnvironment {
    if let Some(mut resolved) = compilation.environment.clone() {
        if settings.ui.is_some() {
            resolved.ui = settings.ui.map(str::to_owned);
        }
        if settings.backend == Backend::Rt {
            resolved.tags.insert("rt".into());
            resolved.tags.remove("effect");
        } else {
            resolved.tags.insert("effect".into());
            resolved.tags.remove("rt");
        }
        if settings.effect_bridge.available() {
            resolved.tags.insert("effect-adapter".into());
        } else {
            resolved.tags.remove("effect-adapter");
        }
        return resolved;
    }
    let (backend, platform, explicit_environment) = match settings.environment {
        Some(env) => (env.backend.clone(), env.platform.clone(), true),
        None => ("js".to_owned(), "node".to_owned(), false),
    };
    let mut tags = BTreeSet::new();
    tags.insert(backend.clone());
    tags.insert(platform.clone());
    if backend == "js" {
        tags.insert(if settings.backend == Backend::Rt {
            "rt".to_owned()
        } else {
            "effect".to_owned()
        });
        if settings.effect_bridge.available() {
            tags.insert("effect-adapter".to_owned());
        }
    }
    ResolvedEnvironment {
        backend_id: format!("flick/{backend}"),
        backend,
        platform,
        host_package: None,
        tags,
        explicit_environment,
        ui: settings.ui.map(str::to_owned),
    }
}
