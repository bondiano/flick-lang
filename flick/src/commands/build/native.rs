//! Temporary native baseline for projects without a component pin.

use super::js_artifacts::js_artifacts;
use super::{BuildPlan, Emission, backend};
use crate::artifact::Artifact;
use crate::sources::FsHost;
use flick_core::build::{self, Settings};
use flick_core::project::config::Backend;
use flick_core::project::{self, Compilation, Project, js};
use flick_core::source::{FileId, SourceMap};
use std::fs;
use std::path::{Path, PathBuf};

pub fn artifacts(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Emission {
    let out_dir = output.out_dir.as_path();
    let modules = &compilation.modules;
    let package = project.and_then(|p| project::project_name(host, &p.root));
    let paths: Vec<(FileId, PathBuf)> = modules.iter().map(|m| (m.file, m.path.clone())).collect();
    let source_path = |file: FileId| {
        paths
            .iter()
            .find(|(f, _)| *f == file)
            .map_or_else(String::new, |(_, p)| p.display().to_string())
    };
    let dirs: Vec<(String, PathBuf)> = modules
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                m.path.parent().map(Path::to_path_buf).unwrap_or_default(),
            )
        })
        .collect();
    let source_dir = |module: &str| {
        dirs.iter()
            .find(|(name, _)| name == module)
            .map(|(_, dir)| dir.clone())
    };
    let runners: Vec<(String, String)> = project
        .iter()
        .flat_map(|p| &p.config.entries)
        .filter_map(|e| Some((e.name.clone(), e.runner.as_ref()?.0.clone())))
        .collect();
    let backend = if output.extras.rt {
        Backend::Rt
    } else {
        project.map_or(Backend::Effect, |p| p.config.build.effect)
    };

    let settings = Settings {
        source_dir: &source_dir,
        out_dir,
        package: package.as_deref(),
        entries: &output.entries,
        typescript: output.typescript,
        source_path: &source_path,
        extras: output.extras,
        release: output.release,
        root: project.map(|p| p.root.as_path()),
        backend,
        effect_bridge: backend::effect_bridge(host, project, compilation),
        ui: project.and_then(|p| p.config.build.ui.as_deref()),
        runners: &runners,
    };
    if output.go {
        let (artifacts, diagnostics) =
            build::go::emit(compilation, sources, &settings, &output.suites, &|path| {
                fs::read_to_string(path).ok()
            });
        return Emission {
            artifacts: artifacts.into_iter().map(Artifact::from).collect(),
            diagnostics,
        };
    }
    let mut emission = js_artifacts(sources, compilation, &settings);
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
