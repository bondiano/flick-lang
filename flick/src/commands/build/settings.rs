//! Shared host preparation of native and component frontend projections.

use super::{BuildPlan, backend};
use crate::sources::FsHost;
use flick_core::build::Settings;
use flick_core::project::config::Backend;
use flick_core::project::{self, Compilation, Project};
use flick_core::source::FileId;
use std::path::{Path, PathBuf};

pub(crate) fn with_settings<R>(
    host: &FsHost,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
    prepare: impl FnOnce(&Settings<'_>) -> R,
) -> R {
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
    prepare(&settings)
}
