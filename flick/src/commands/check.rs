use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::{Diagnostic, Edit, Severity, codes};
use flick_core::project::{self, ModuleCache, Project};
use flick_core::source::{FileId, SourceMap};

use crate::args::CheckArguments;
use crate::command::{Command, Outcome};
use crate::sources::{self, FsHost};
use crate::{builtins, package_cache, timings};

mod pages;

pub struct CheckCommand {
    options: CheckArguments,
    project: Option<String>,
    /// What the checks before this one checked, for a command that checks
    /// again: the passes of `--fix` and the runs of `--watch`.
    modules: RefCell<ModuleCache>,
}

impl CheckCommand {
    pub fn new(options: CheckArguments, project: Option<String>) -> Self {
        Self {
            options,
            project,
            modules: RefCell::default(),
        }
    }
}

impl Command for CheckCommand {
    fn run(&self) -> Outcome {
        let (pages, paths): (Vec<String>, Vec<String>) =
            (self.options.paths.iter().cloned()).partition(|p| pages::is_page(p));
        // Markdown pages alone check no Flick sources, not the whole project
        let mut outcome = if paths.is_empty() && !pages.is_empty() {
            Outcome::default()
        } else {
            self.check_files(&paths)
        };
        if !pages.is_empty()
            && let Err(diagnostic) = builtins::prepare()
        {
            outcome.diagnostics.push(diagnostic);
            return outcome;
        }
        for page in &pages {
            let fix = self.options.fix.then_some(self.options.only.as_slice());
            let diagnostics = pages::check(&mut outcome.sources, page, fix);
            outcome.diagnostics.extend(diagnostics);
        }
        outcome
    }
}

/// How many times `--fix` checks and fixes again at most.
const FIX_PASSES: usize = 10;

impl CheckCommand {
    /// The diagnostics of the Flick files of `paths`, fixed first with `--fix`.
    fn check_files(&self, paths: &[String]) -> Outcome {
        let host = FsHost::new();
        let (mut outcome, mut roots) = self.check(&host, paths);
        if !self.options.fix {
            return outcome;
        }
        // a fix left out for an overlap, or one a fix uncovers, waits for the next pass
        let mut errors = Vec::new();
        for _ in 0..FIX_PASSES {
            let (changed, failed) = apply_fixes(&outcome, &roots, &host.cwd, &self.options.only);
            errors.extend(failed);
            if !changed {
                break;
            }
            (outcome, roots) = self.check(&host, paths);
        }
        outcome.diagnostics.extend(errors);
        outcome
    }

    /// The diagnostics of the files selected by `paths`, and those files.
    fn check(&self, host: &FsHost, paths: &[String]) -> (Outcome, Vec<PathBuf>) {
        let mut sources = SourceMap::default();
        timings::stage("Resolving project");
        let (project, roots) = match select(host, &mut sources, self.project.as_deref(), paths) {
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
        // Projects select class-derived stages on RT. Persist that set of
        // builtin interfaces, rather than checking std anew in every process.
        let rt = project::stages_by_class(project.as_ref(), false);
        timings::stage("Preparing compiler");
        if let Err(diagnostic) = builtins::prepare_in(rt) {
            return (
                Outcome {
                    sources,
                    diagnostics: vec![diagnostic],
                },
                roots,
            );
        }
        timings::stage("Checking Flick modules");
        // one check keeps nothing in memory, which no later check reads; it
        // saves its modules for the next process instead
        let again = self.options.watch || self.options.fix;
        let mut modules = self.modules.borrow_mut();
        let kept = again.then_some(&mut *modules);
        let diagnostics = package_cache::with_modules(kept, |kept| {
            package_cache::with_saved(|saved| {
                project::check_with(host, &mut sources, project.as_ref(), &roots, saved, kept)
            })
        });
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
fn apply_fixes(
    outcome: &Outcome,
    roots: &[PathBuf],
    cwd: &Path,
    only: &[String],
) -> (bool, Vec<Diagnostic>) {
    let mut by_file: BTreeMap<FileId, Vec<&[Edit]>> = BTreeMap::new();
    let fixes = (outcome.diagnostics.iter())
        .filter(|d| chosen(d, only))
        .flat_map(|d| &d.fixes);
    for fix in fixes.filter(|f| f.safe) {
        if let Some(first) = fix.edits.first() {
            by_file.entry(first.span.file).or_default().push(&fix.edits);
        }
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

/// Whether `--only` lets the fixes of `d` through: `E002` or `FLICK-E002`.
pub(super) fn chosen(d: &Diagnostic, only: &[String]) -> bool {
    let code = d.code.to_string();
    only.is_empty() || (only.iter()).any(|c| code == *c || code.strip_prefix("FLICK-") == Some(c))
}

/// `text` with `fixes`, each made whole or not at all.
fn fixed(text: &str, fixes: Vec<&[Edit]>) -> String {
    let mut out = text.to_owned();
    for edit in taken(fixes) {
        out.replace_range(edit.span.range(), &edit.replacement);
    }
    out
}

/// The edits of `fixes`, last first, each fix whole or not at all: one with
/// an edit that overlaps an edit of a fix already taken is left out.
fn taken(fixes: Vec<&[Edit]>) -> Vec<&Edit> {
    let overlaps = |a: &Edit, b: &Edit| {
        a.span.start < b.span.end && b.span.start < a.span.end
            || a.span.start == b.span.start && a.span.end == b.span.end
    };
    let mut taken: Vec<&Edit> = Vec::new();
    for fix in fixes {
        if !fix.iter().any(|e| taken.iter().any(|t| overlaps(e, t))) {
            taken.extend(fix);
        }
    }
    taken.sort_by_key(|e| Reverse((e.span.start, e.span.end)));
    taken
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
    use std::slice;

    #[test]
    fn fixed_takes_each_fix_whole_or_not_at_all() {
        let file = SourceMap::default().add("a.fl", "");
        let edit = |start, end, replacement: &str| Edit {
            span: Span::new(file, start, end),
            replacement: replacement.to_owned(),
        };
        let (a, b, c) = (edit(5, 9, ""), edit(19, 23, ""), edit(17, 20, "x"));
        let text = "let a : T = 1\nlet b : T = 2";
        assert_eq!(
            fixed(
                text,
                vec![
                    slice::from_ref(&a),
                    slice::from_ref(&c),
                    slice::from_ref(&b)
                ]
            ),
            "let a = 1\nletx: T = 2"
        );
        // the fix of `c` and `a` overlaps the one of `b`, so none of it is made
        assert_eq!(fixed(text, vec![&[b], &[c, a]]), "let a : T = 1\nlet b = 2");
    }
}
