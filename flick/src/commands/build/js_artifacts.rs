//! The artifacts of a JavaScript build.

use flick_core::build::{self, Artifact, Extras, Settings};
use flick_core::project::Compilation;
use flick_core::project::config::Backend;
use flick_core::source::SourceMap;

use super::{Emission, shipped};

/// The output of a js build; a library on Effect is also built without it,
/// for applications with `effect = "none"`.
pub(super) fn js_artifacts(
    sources: &SourceMap,
    library: bool,
    compilation: &Compilation,
    settings: &Settings,
) -> Emission {
    let (shipped, problems) = shipped::foreign_files(compilation, &|_| true);
    if !library {
        let (mut artifacts, mut diagnostics) = build::emit(compilation, sources, settings);
        artifacts.extend(shipped);
        diagnostics.extend(problems);
        return Emission {
            artifacts,
            diagnostics,
        };
    }
    let core_dir = settings.out_dir.join(build::effects::CORE_DIR);
    let (core, _) = build::emit(
        compilation,
        sources,
        &Settings {
            out_dir: &core_dir,
            backend: Backend::None,
            extras: Extras {
                labels: settings.extras.labels,
                ..Extras::default()
            },
            core_only: true,
            ..*settings
        },
    );
    let core_modules = build::core_built(&core, compilation, settings.typescript);
    let (mut artifacts, mut diagnostics) = build::emit(
        compilation,
        sources,
        &Settings {
            core_modules: &core_modules,
            ..*settings
        },
    );
    artifacts.extend(shipped);
    diagnostics.extend(problems);
    let (core_shipped, _) =
        shipped::foreign_files(compilation, &|m| core_modules.iter().any(|c| c == m));
    artifacts.extend(core.into_iter().chain(core_shipped).map(|a| Artifact {
        path: format!("{}{}", build::effects::CORE_DIR, a.path),
        contents: a.contents,
    }));
    Emission {
        artifacts,
        diagnostics,
    }
}
