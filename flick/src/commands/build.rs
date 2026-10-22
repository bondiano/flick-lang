use std::path::{Path, PathBuf};
use std::{fs, io};

use flick_core::build::{self, Settings};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::{Emit, Kind};
use flick_core::project::{self, Compiled};
use flick_core::source::{FileId, SourceMap};

use super::check::select;
use crate::args::{BuildArguments, EmitArgument};
use crate::command::{Command, Outcome};
use crate::sources::FsHost;

pub struct BuildCommand {
    options: BuildArguments,
    project: Option<String>,
}

impl BuildCommand {
    pub fn new(options: BuildArguments, project: Option<String>) -> Self {
        Self { options, project }
    }
}

impl Command for BuildCommand {
    fn run(&self) -> Outcome {
        let mut sources = SourceMap::default();
        let host = FsHost::new();
        let selected = select(
            &host,
            &mut sources,
            self.project.as_deref(),
            &self.options.paths,
        );
        let (project, roots) = match selected {
            Ok(selected) => selected,
            Err(diagnostic) => {
                return Outcome {
                    sources,
                    diagnostics: vec![diagnostic],
                };
            }
        };
        let (mut diagnostics, mut compilation) =
            project::compile(&host, &mut sources, project.as_ref(), &roots);
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return Outcome {
                sources,
                diagnostics,
            };
        }
        compilation.modules.retain(|c| !c.test);
        let modules = &compilation.modules;
        let entry = match &project {
            Some(p) if p.config.kind == Kind::App => p.config.entry.as_ref().map(|e| e.0.clone()),
            Some(_) => None,
            None => {
                let mains: Vec<&Compiled> = modules
                    .iter()
                    .filter(|m| roots.contains(&m.path) && m.interface.exports_value("main"))
                    .collect();
                match mains[..] {
                    [only] => Some(only.name.clone()),
                    _ => None,
                }
            }
        };
        let out_dir = match (&self.options.out_dir, &project) {
            (Some(dir), _) => host.cwd.join(dir),
            (None, Some(p)) => p.root.join(&p.config.build.out_dir),
            (None, None) => host.cwd.join("dist"),
        };
        let typescript = match (self.options.emit, &project) {
            (Some(emit), _) => emit == EmitArgument::Ts,
            (None, Some(p)) => p.config.build.emit == Emit::Ts,
            (None, None) => false,
        };
        let package = project
            .as_ref()
            .and_then(|p| project::project_name(&host, &p.root));
        let paths: Vec<(FileId, PathBuf)> =
            modules.iter().map(|m| (m.file, m.path.clone())).collect();
        let source_path = |file: FileId| {
            paths
                .iter()
                .find(|(f, _)| *f == file)
                .map_or_else(String::new, |(_, p)| format!("file://{}", p.display()))
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
        let settings = Settings {
            source_dir: &source_dir,
            out_dir: &out_dir,
            package: package.as_deref(),
            entry: entry.as_deref(),
            typescript,
            source_path: &source_path,
        };
        for artifact in build::emit(&compilation, &sources, &settings) {
            let path = out_dir.join(&artifact.path);
            if let Err(err) = write(&path, &artifact.contents) {
                diagnostics.push(Diagnostic::new(
                    codes::P013,
                    Severity::Error,
                    format!("cannot write `{}`: {err}", path.display()),
                ));
                break;
            }
        }
        Outcome {
            sources,
            diagnostics,
        }
    }
}

fn write(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)
}
