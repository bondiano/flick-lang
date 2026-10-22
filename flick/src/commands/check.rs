use std::path::{Path, PathBuf};

use flick_core::project;
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
        let explicit = self.project.as_deref().map(Path::new);
        let project = match project::discover(&host, &mut sources, &host.cwd, explicit) {
            Ok(project) => project,
            Err(diagnostic) => {
                return Outcome {
                    sources,
                    diagnostics: vec![diagnostic],
                };
            }
        };
        let mut files = Vec::new();
        if self.options.paths.is_empty() {
            let Some(project) = &project else {
                let message = format!(
                    "no `{}` in the working directory or any parent",
                    project::CONFIG_FILE
                );
                return Outcome {
                    sources,
                    diagnostics: vec![project::no_project(message)],
                };
            };
            for root in [project.source_root(), project.test_root()] {
                if root.is_dir() {
                    sources::source_files(&root.to_string_lossy(), &mut files);
                }
            }
        } else {
            for path in &self.options.paths {
                sources::source_files(path, &mut files);
            }
        }
        let roots: Vec<PathBuf> = files
            .iter()
            .map(|f| project::normalize(&host.cwd.join(f)))
            .collect();
        let diagnostics = project::check(&host, &mut sources, project.as_ref(), &roots);
        Outcome {
            sources,
            diagnostics,
        }
    }
}
