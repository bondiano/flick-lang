//! Temporary native baseline for projects without a component pin.

use super::js_artifacts::js_artifacts;
use super::{BuildPlan, Emission};
use crate::artifact::Artifact;
use crate::sources::FsHost;
use flick_core::build::{self, Settings};
use flick_core::project::{Compilation, Project, js};
use flick_core::source::SourceMap;
use std::fs;

pub fn artifacts(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Emission {
    super::settings::with_settings(host, project, compilation, output, |settings| {
        artifacts_with(host, sources, project, compilation, output, settings)
    })
}

fn artifacts_with(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
    settings: &Settings<'_>,
) -> Emission {
    if output.go {
        let (artifacts, diagnostics) =
            build::go::emit(compilation, sources, settings, &output.suites, &|path| {
                fs::read_to_string(path).ok()
            });
        return Emission {
            artifacts: artifacts.into_iter().map(Artifact::from).collect(),
            diagnostics,
        };
    }
    let mut emission = js_artifacts(sources, compilation, settings);
    if let Some(project) = project {
        let wanted = &compilation.js_dependencies;
        let needs = &mut emission.diagnostics;
        needs.extend(js::conflicts(host, &project.root, wanted));
        needs.extend(js::unmet(host, &project.root, wanted));
    } else {
        // files built alone have no manifest of the outputs: the list names
        // them, so the next build removes those it does not write
        let contents = (emission.artifacts.iter()).fold(String::new(), |mut list, a| {
            list.push_str(&a.path);
            list.push('\n');
            list
        });
        emission
            .artifacts
            .push(Artifact::source(build::go::FILES.to_owned(), contents));
    }
    emission
}
