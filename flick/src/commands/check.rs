use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::{Diagnostic, Edit, Severity, codes};
use flick_core::project::{self, Project};
use flick_core::source::{FileId, SourceMap};

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
        let host = FsHost::new();
        let (outcome, roots) = self.check(&host);
        if !self.options.fix {
            return outcome;
        }
        let (changed, errors) = apply_fixes(&outcome, &roots, &host.cwd);
        let mut outcome = if changed {
            self.check(&host).0
        } else {
            outcome
        };
        outcome.diagnostics.extend(errors);
        outcome
    }
}

impl CheckCommand {
    /// The diagnostics of the selected files, and those files.
    fn check(&self, host: &FsHost) -> (Outcome, Vec<PathBuf>) {
        let mut sources = SourceMap::default();
        let (project, roots) = match select(
            host,
            &mut sources,
            self.project.as_deref(),
            &self.options.paths,
        ) {
            Ok(selected) => selected,
            Err(diagnostic) => {
                let diagnostics = vec![diagnostic];
                return (
                    Outcome {
                        sources,
                        diagnostics,
                    },
                    Vec::new(),
                );
            }
        };
        let diagnostics = project::check(host, &mut sources, project.as_ref(), &roots);
        (
            Outcome {
                sources,
                diagnostics,
            },
            roots,
        )
    }
}

/// Writes the safe fixes of `outcome` into those of its files that are
/// `roots`, never into a dependency; whether a file changed, and the files
/// that could not be written.
fn apply_fixes(outcome: &Outcome, roots: &[PathBuf], cwd: &Path) -> (bool, Vec<Diagnostic>) {
    let mut by_file: BTreeMap<FileId, Vec<&Edit>> = BTreeMap::new();
    let fixes = outcome.diagnostics.iter().flat_map(|d| &d.fixes);
    for edit in fixes.filter(|f| f.safe).flat_map(|f| &f.edits) {
        by_file.entry(edit.span.file).or_default().push(edit);
    }
    let mut changed = false;
    let mut errors = Vec::new();
    for (file, edits) in by_file {
        let source = outcome.sources.get(file);
        let path = project::normalize(&cwd.join(&source.path));
        if !roots.contains(&path) {
            continue;
        }
        match fs::write(&path, fixed(&source.text, edits)) {
            Ok(()) => changed = true,
            Err(err) => errors.push(Diagnostic::new(
                codes::P004,
                Severity::Error,
                format!("cannot write `{}`: {err}", source.path),
            )),
        }
    }
    (changed, errors)
}

/// `text` with `edits`, the last first; an edit that overlaps one already
/// made is left out.
// shortcut: a fix of several edits could lose one to an overlap; safe fixes have one edit each
fn fixed(text: &str, mut edits: Vec<&Edit>) -> String {
    edits.sort_by_key(|e| Reverse((e.span.start, e.span.end)));
    let mut out = text.to_owned();
    let mut before = u32::MAX;
    for edit in edits {
        if edit.span.end <= before {
            out.replace_range(edit.span.range(), &edit.replacement);
            before = edit.span.start;
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use flick_core::source::Span;

    #[test]
    fn fixed_applies_edits_from_the_end_and_skips_overlaps() {
        let file = SourceMap::default().add("a.fl", "");
        let edit = |start, end, replacement: &str| Edit {
            span: Span::new(file, start, end),
            replacement: replacement.to_owned(),
        };
        let (a, b, c) = (edit(5, 9, ""), edit(19, 23, ""), edit(17, 20, "x"));
        assert_eq!(
            fixed("let a : T = 1\nlet b : T = 2", vec![&a, &c, &b]),
            "let a = 1\nlet b = 2"
        );
    }
}
