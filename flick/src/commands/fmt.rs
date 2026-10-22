use std::io::Write;

use clap::CommandFactory;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::{FileId, SourceMap, Span};

use crate::args::{CommandArguments, FmtArguments};
use crate::command::{Command, Outcome};
use crate::sources;

pub struct FmtCommand {
    options: FmtArguments,
}

impl FmtCommand {
    /// Exits with a usage error (code 2) for `--stdin-filepath` without
    /// `--stdin`: clap's `requires` counts the defaulted bool flag as present.
    pub fn new(options: FmtArguments) -> Self {
        if options.stdin_filepath.is_some() && !options.stdin {
            CommandArguments::command()
                .error(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "`--stdin-filepath` requires `--stdin`",
                )
                .exit();
        }
        Self { options }
    }

    fn files(&self) -> Outcome {
        let mut paths = Vec::new();
        for path in &self.options.paths {
            sources::source_files(path, &mut paths);
        }
        let mut outcome = Outcome::default();
        for path in paths {
            let file = match sources::load(&mut outcome.sources, &path) {
                Ok(file) => file,
                Err(error) => {
                    outcome.diagnostics.push(error);
                    continue;
                }
            };
            let text = &outcome.sources.get(file).text;
            match flick_core::fmt::format(text, file) {
                Ok(out) if out == *text => {}
                Ok(out) if self.options.check => {
                    let unformatted = not_formatted(&outcome.sources, file, &out);
                    outcome.diagnostics.push(unformatted);
                }
                Ok(out) => {
                    if let Err(err) = std::fs::write(&path, out) {
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
        let text = &outcome.sources.get(file).text;
        match flick_core::fmt::format(text, file) {
            Ok(out) if self.options.check => {
                if out != *text {
                    let unformatted = not_formatted(&outcome.sources, file, &out);
                    outcome.diagnostics.push(unformatted);
                }
            }
            Ok(out) => {
                // stdout closed by the editor: nothing useful left to do
                let _ = std::io::stdout().lock().write_all(out.as_bytes());
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
