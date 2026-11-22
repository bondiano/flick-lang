//! The native emitters, linked for development and parity only: a release
//! build always runs a backend component.

#[cfg(any(test, feature = "native-backends-development"))]
use super::Purpose;
#[cfg(any(test, feature = "native-backends-development"))]
use super::js_artifacts::js_artifacts;
use super::{BuildPlan, Emission};
#[cfg(any(test, feature = "native-backends-development"))]
use crate::artifact::Artifact;
#[cfg(all(
    any(test, feature = "native-backends-development"),
    feature = "wasm-backends"
))]
use crate::backends;
#[cfg(not(any(test, feature = "native-backends-development")))]
use crate::backends::failure;
use crate::sources::FsHost;
#[cfg(any(test, feature = "native-backends-development"))]
use flick_core::build;
use flick_core::build::Settings;
#[cfg(all(
    any(test, feature = "native-backends-development"),
    not(feature = "wasm-backends")
))]
use flick_core::project::Host;
#[cfg(any(test, feature = "native-backends-development"))]
use flick_core::project::target;
use flick_core::project::{Compilation, Project};
use flick_core::source::SourceMap;
#[cfg(any(test, feature = "native-backends-development"))]
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
    #[cfg(any(test, feature = "native-backends-development"))]
    if output.go {
        let (artifacts, diagnostics) = build::go::emit(
            compilation,
            sources,
            settings,
            &output.suites,
            output.purpose == Purpose::Test,
            &|path| fs::read_to_string(path).ok(),
        );
        return Emission {
            artifacts: artifacts.into_iter().map(Artifact::from).collect(),
            diagnostics,
        };
    }
    #[cfg(not(any(test, feature = "native-backends-development")))]
    {
        let _ = (host, sources, project, compilation, output, settings);
        Emission {
            artifacts: vec![],
            diagnostics: vec![failure(
                "direct Rust emission is available only for development/parity",
            )],
        }
    }
    #[cfg(any(test, feature = "native-backends-development"))]
    {
        let mut emission = js_artifacts(sources, compilation, settings);
        if let Some(project) = project {
            let wanted = &compilation.target_dependencies;
            let needs = &mut emission.diagnostics;
            needs.extend(target::conflicts(wanted));
            #[cfg(feature = "wasm-backends")]
            {
                let _ = host;
                needs.extend(backends::verify_for_target(
                    project,
                    Some(&output.target),
                    wanted,
                    true,
                ));
            }
            #[cfg(not(feature = "wasm-backends"))]
            needs.extend(host.verify(project, wanted));
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
}
