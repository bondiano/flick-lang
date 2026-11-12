//! The artifacts of a JavaScript build.

use flick_core::build::{self, Settings};
use flick_core::project::Compilation;
use flick_core::source::SourceMap;

use super::{Emission, shipped};

/// The output of a js build and the foreign files it ships.
pub(super) fn js_artifacts(
    sources: &SourceMap,
    compilation: &Compilation,
    settings: &Settings,
) -> Emission {
    let (shipped, problems) = shipped::foreign_files(compilation, &|_| true);
    let (mut artifacts, mut diagnostics) = build::emit(compilation, sources, settings);
    artifacts.extend(shipped);
    diagnostics.extend(problems);
    Emission {
        artifacts,
        diagnostics,
    }
}
