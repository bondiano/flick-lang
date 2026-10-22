use std::path::{Path, PathBuf};

use flick_core::diagnostics::Diagnostic;
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;

use crate::args::CheckArguments;
use crate::command::{Command, Outcome};
use crate::sources::{self, FsHost};

pub struct CheckCommand {
    options: CheckArguments,
    project: Option<String>,
}

impl CheckCommand {
    pub fn new(options: CheckArguments, project: Option<String>) -> Self {
        Self { options, project }
    }
}

impl Command for CheckCommand {
    fn run(&self) -> Outcome {
        let mut sources = SourceMap::default();
        let host = FsHost::new();
        let (project, roots) = match select(
            &host,
            &mut sources,
            self.project.as_deref(),
            &self.options.paths,
        ) {
            Ok(selected) => selected,
            Err(diagnostic) => {
                return Outcome {
                    sources,
                    diagnostics: vec![diagnostic],
                };
            }
        };
        let diagnostics = project::check(&host, &mut sources, project.as_ref(), &roots);
        Outcome {
            sources,
            diagnostics,
        }
    }
}

/// The project and the root files a command works on: `paths`, or without
/// them the project's source and test roots.
pub fn select(
    host: &FsHost,
    sources: &mut SourceMap,
    explicit: Option<&str>,
    paths: &[String],
) -> Result<(Option<Project>, Vec<PathBuf>), Diagnostic> {
    let project = project::discover(host, sources, &host.cwd, explicit.map(Path::new))?;
    let mut files = Vec::new();
    if paths.is_empty() {
        let Some(project) = &project else {
            let message = format!(
                "no `{}` in the working directory or any parent",
                project::CONFIG_FILE
            );
            return Err(project::no_project(message));
        };
        for root in [project.source_root(), project.test_root()] {
            if root.is_dir() {
                sources::source_files(&root.to_string_lossy(), &mut files);
            }
        }
    } else {
        for path in paths {
            sources::source_files(path, &mut files);
        }
    }
    let roots = files
        .iter()
        .map(|f| project::normalize(&host.cwd.join(f)))
        .collect();
    Ok((project, roots))
}
