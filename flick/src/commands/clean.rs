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
use crate::cache::manifest;
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

/// Removes the directories of `path` up to `top` that are left empty.
fn remove_empty_parents(path: &Path, top: &Path) {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == top || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}
