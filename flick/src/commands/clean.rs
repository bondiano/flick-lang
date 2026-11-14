//! `flick clean`: removes the files the manifest lists as written by
//! builds, the test build and the generations no process holds; files of
//! others in a shared output directory stay. `--cache` also removes the
//! build cache.

use std::fs;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::cache;
use flick_core::source::SourceMap;

use super::run::{find_project, generations, locked};
use crate::args::{CleanArguments, GlobalArguments};
use crate::artifact::{resolve_root, validate_root};
use crate::cache::{inputs_path, remove_recorded};
use crate::command::Outcome;
use crate::report;

pub fn clean(options: &CleanArguments, global: &GlobalArguments) -> Outcome {
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    let root = resolve_root(&project.root, &project.root);
    let root = root.as_path();
    // These directories are host-owned even when the manifest is empty.
    // Check all recursive removals before removing any recorded output.
    for path in ["test", "run"]
        .into_iter()
        .chain(options.cache.then_some("cache"))
    {
        if let Err(why) = validate_root(&root.join(cache::DIR).join(path)) {
            return Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![Diagnostic::new(codes::P013, Severity::Error, why)],
            };
        }
    }
    let removed = match remove_recorded(root) {
        Ok(removed) => removed,
        Err(error) => {
            return Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![Diagnostic::new(
                    codes::P013,
                    Severity::Error,
                    format!("cannot clean recorded build outputs: {error}"),
                )],
            };
        }
    };
    let _ = fs::remove_file(root.join(cache::MANIFEST));
    let _ = fs::remove_dir_all(root.join(cache::DIR).join("test"));
    let kept = clean_generations(root);
    if options.cache {
        let _ = fs::remove_dir_all(root.join(cache::DIR).join("cache"));
        let _ = fs::remove_file(inputs_path(root));
    }
    let note = if kept > 0 {
        format!("clean: removed {removed} files; kept {kept} running generations")
    } else {
        format!("clean: removed {removed} files")
    };
    report::log(global, &note);
    Outcome {
        sources: SourceMap::default(),
        diagnostics: Vec::new(),
    }
}

/// Keeps both construction reservations and running or orphaned generations.
pub(super) fn clean_generations(root: &Path) -> usize {
    let mut kept = 0;
    if let Ok(entries) = fs::read_dir(generations(root)) {
        for generation in entries.flatten().map(|e| e.path()) {
            if locked(&generation) {
                kept += 1;
            } else {
                let _ = fs::remove_dir_all(&generation);
            }
        }
    }
    kept
}
