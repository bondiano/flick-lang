//! The output directory of a build: one that would take the place of the
//! project's own files is refused.

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;

/// Why `out_dir` may not take the build: it is the project's root or the
/// directory of its sources or tests, or above one of them, whose files a
/// build or `clean` would replace.
pub fn overlap(project: &Project, out_dir: &Path) -> Option<Diagnostic> {
    let out = canonical(out_dir);
    let root = canonical(&project.root);
    let within = |a: &Path, b: &Path| a.starts_with(b);
    let reason = if within(&root, &out) {
        "the project's root or a directory above it"
    } else if [project.source_root(), project.test_root()]
        .iter()
        .map(|dir| canonical(dir))
        .any(|dir| within(&dir, &out))
    {
        "the directory of its sources or tests, or one above it"
    } else {
        return None;
    };
    Some(Diagnostic::new(
        codes::P028,
        Severity::Error,
        format!(
            "the output directory `{}` is {reason}: a build writes its own `package.json` there and `flick clean` removes it; set `build.out-dir` or `--out-dir` to a directory of its own, such as `dist`",
            out_dir.display()
        ),
    ))
}

/// `path` with symbolic links resolved, as far as it exists.
fn canonical(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path.to_path_buf();
    loop {
        if let Ok(found) = fs::canonicalize(&existing) {
            return missing.iter().rev().fold(found, |acc, part| acc.join(part));
        }
        match (
            existing.file_name().map(ToOwned::to_owned),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}
