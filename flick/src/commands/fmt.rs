use std::io::Write;
use std::path::Path;
use std::{fs, io};

use clap::CommandFactory;
use clap::error::ErrorKind;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::fmt;
use flick_core::project::{self, FormatPlugins, Project};
use flick_core::source::{FileId, SourceMap, Span};

use crate::args::{CommandArguments, FmtArguments};
use crate::command::{Command, Outcome};
use crate::sources::{self, FsHost};

pub struct FmtCommand {
    options: FmtArguments,
    project: Option<String>,
}

impl FmtCommand {
    /// Exits with a usage error (code 2) for `--stdin-filepath` without
    /// `--stdin`: clap's `requires` counts the defaulted bool flag as present.
    pub fn new(options: FmtArguments, project: Option<String>) -> Self {
        if options.stdin_filepath.is_some() && !options.stdin {
            CommandArguments::command()
                .error(
                    ErrorKind::MissingRequiredArgument,
                    "`--stdin-filepath` requires `--stdin`",
                )
                .exit();
        }
        Self { options, project }
    }

    fn files(&self) -> Outcome {
        let mut outcome = Outcome::default();
        let mut paths = Vec::new();
        let host = sources::FsHost::new();
        let project = if self.options.paths.is_empty() {
            // without paths: the project's source and test roots
            match super::check::select(&host, &mut outcome.sources, self.project.as_deref(), &[]) {
                Ok((project, roots)) => {
                    paths.extend(roots.iter().map(|r| r.to_string_lossy().into_owned()));
                    project
                }
                Err(d) => {
                    outcome.diagnostics.push(d);
                    return outcome;
                }
            }
        } else {
            self.project_at(&host, &mut outcome.sources, &host.cwd)
        };
        let mut plugins = FormatPlugins::new(&host, &mut outcome.sources, project.as_ref());
        for path in &self.options.paths {
            sources::source_files(path, &mut paths);
        }
        for path in paths {
            let file = match sources::load(&mut outcome.sources, &path) {
                Ok(file) => file,
                Err(error) => {
                    outcome.diagnostics.push(error);
                    continue;
                }
            };
            let text = &outcome.sources.get(file).text;
            let found = plugins.of(text, file);
            match fmt::format_with(text, file, &|head| found.get(head).cloned()) {
                Ok(out) if out == *text => {}
                Ok(out) if self.options.check => {
                    let unformatted = not_formatted(&outcome.sources, file, &out);
                    outcome.diagnostics.push(unformatted);
                }
                Ok(out) => {
                    if let Err(err) = fs::write(&path, out) {
                        outcome.diagnostics.push(Diagnostic::new(
                            codes::P004,
                            Severity::Error,
                            format!("cannot write `{path}`: {err}"),
                        ));
                    }
                }
                Err(errors) => outcome.diagnostics.extend(errors),
            }
        }
        outcome
    }

    /// The project at `dir` whose packages' plugins lay out blocks, when
    /// there is one; a broken manifest leaves blocks as written, for
    /// `flick check` to report.
    fn project_at(&self, host: &FsHost, sources: &mut SourceMap, dir: &Path) -> Option<Project> {
        let explicit = self.project.as_deref().map(Path::new);
        project::discover(host, sources, dir, explicit)
            .ok()
            .flatten()
    }

    fn stdin(&self) -> Outcome {
        let mut outcome = Outcome::default();
        let path = self.options.stdin_filepath.as_deref().unwrap_or("<stdin>");
        let file = match sources::load_stdin(&mut outcome.sources, path) {
            Ok(file) => file,
            Err(error) => {
                outcome.diagnostics.push(error);
                return outcome;
            }
        };
        let host = sources::FsHost::new();
        let at = host.cwd.join(path);
        let dir = at
            .parent()
            .map_or_else(|| host.cwd.clone(), Path::to_path_buf);
        let project = self.project_at(&host, &mut outcome.sources, &dir);
        let mut plugins = FormatPlugins::new(&host, &mut outcome.sources, project.as_ref());
        let text = &outcome.sources.get(file).text;
        let found = plugins.of(text, file);
        match fmt::format_with(text, file, &|head| found.get(head).cloned()) {
            Ok(out) if self.options.check => {
                if out != *text {
                    let unformatted = not_formatted(&outcome.sources, file, &out);
                    outcome.diagnostics.push(unformatted);
                }
            }
            Ok(out) => {
                // stdout closed by the editor: nothing useful left to do
                let _ = io::stdout().lock().write_all(out.as_bytes());
            }
            Err(errors) => outcome.diagnostics = errors,
        }
        outcome
    }
}

impl Command for FmtCommand {
    fn run(&self) -> Outcome {
        if self.options.stdin {
            self.stdin()
        } else {
            self.files()
        }
    }
}

/// `P002` pointing at the first line that formatting changes.
fn not_formatted(sources: &SourceMap, file: FileId, formatted: &str) -> Diagnostic {
    let text = &sources.get(file).text;
    let differs = text
        .bytes()
        .zip(formatted.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(text.len().min(formatted.len()));
    let start = text[..differs].rfind('\n').map_or(0, |i| i + 1);
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let offset = |n: usize| u32::try_from(n).expect("source offsets fit in u32");
    Diagnostic::error(
        codes::P002,
        format!("`{}` is not formatted", sources.get(file).path),
        Span::new(file, offset(start), offset(end)),
    )
}
