//! Build-facing compatibility API for project environment preflight.

use super::Settings;
use crate::diagnostics::Diagnostic;
use crate::project::config::BackendEnvironment;
use crate::project::{Compilation, target_preflight};
use crate::source::SourceMap;

pub use target_preflight::{gated_service, handled_services, handles, std_root_handlers};

/// Reject reachable gated effects that the selected environment cannot host.
pub fn check(
    compilation: &Compilation,
    settings: &Settings<'_>,
    sources: &SourceMap,
    environment: &BackendEnvironment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    target_preflight::check(
        compilation,
        settings.entries,
        settings.runners,
        sources,
        environment,
        diagnostics,
    );
}
