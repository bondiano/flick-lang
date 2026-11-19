//! `flick prove`: the proof modules of the project's proof root, checked
//! with the source modules they import. `check`, `build` and `test` never
//! read that root, so code that asks for no proof pays for none.

use std::path::Path;

use flick_core::project;
use flick_core::source::SourceMap;

use crate::sources::FsHost;

/// What `flick prove` checks: `paths`, or the project's proof root. `None`
/// when the project has no proof root: there is nothing to prove.
pub fn proof_paths(paths: &[String], explicit: Option<&str>) -> Option<Vec<String>> {
    if !paths.is_empty() {
        return Some(paths.to_vec());
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    // without a project, checking reports that there is none
    let Ok(Some(project)) =
        project::discover(&host, &mut sources, &host.cwd, explicit.map(Path::new))
    else {
        return Some(Vec::new());
    };
    let root = project.proof_root();
    if !root.is_dir() {
        println!(
            "no proofs: `{}` has no proof modules",
            project.config.proof_dir
        );
        return None;
    }
    Some(vec![root.to_string_lossy().into_owned()])
}
