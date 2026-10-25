//! `flick clean`: removes the files the manifest lists as written by
//! builds, the test build and the generations no process holds; files of
//! others in a shared output directory stay. `--cache` also removes the
//! build cache.

use std::fs;
use std::path::Path;

use flick_core::project::cache;
use flick_core::source::SourceMap;

use super::run::{find_project, generations, locked};
use crate::args::{CleanArguments, GlobalArguments};
use crate::cache::{manifest, remove_empty_parents};
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
    let root = &project.root;
    let mut removed = 0;
    for (dir, files) in manifest(root).outputs {
        let out = root.join(&dir);
        for file in &files {
            let path = out.join(file);
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
            remove_empty_parents(&path, &out);
        }
        let _ = fs::remove_dir(&out);
    }
    let _ = fs::remove_file(root.join(cache::MANIFEST));
    let _ = fs::remove_dir_all(root.join(cache::DIR).join("test"));
    let kept = clean_generations(root);
    if options.cache {
        let _ = fs::remove_dir_all(root.join(cache::DIR).join("cache"));
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
