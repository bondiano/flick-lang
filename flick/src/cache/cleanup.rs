//! Validate every recorded removal before changing a published build.

use std::collections::HashSet;
use std::path::Path;
use std::{fs, io};

use flick_backend::{Limits, validate_output_paths};

use super::{manifest, remove_empty_parents};
use crate::artifact::validate_ancestors;

fn validate_removals(out: &Path, files: &[String]) -> io::Result<()> {
    validate_output_paths(files.iter().map(String::as_str), Limits::default())
        .map_err(io::Error::other)?;
    for file in files {
        validate_ancestors(out, Path::new(file)).map_err(io::Error::other)?;
    }
    Ok(())
}

/// Validate stale paths before any current build files are published.
pub fn validate_record(root: &Path, out: &Path, files: &[String]) -> io::Result<()> {
    validate_output_paths(files.iter().map(String::as_str), Limits::default())
        .map_err(io::Error::other)?;
    let dir = out.strip_prefix(root).unwrap_or(out).to_string_lossy();
    let manifest = manifest(root);
    if let Some((_, previous)) = manifest.outputs.iter().find(|(output, _)| output == &dir) {
        let current: HashSet<&str> = files.iter().map(String::as_str).collect();
        let stale: Vec<_> = previous
            .iter()
            .filter(|file| !current.contains(file.as_str()))
            .cloned()
            .collect();
        validate_removals(out, &stale)?;
    }
    Ok(())
}

/// Preflight the entire manifest, including later output directories.
/// Unlinking a leaf symlink is safe; following a parent symlink is refused.
pub fn remove_recorded(root: &Path) -> io::Result<usize> {
    let outputs = manifest(root).outputs;
    for (dir, files) in &outputs {
        validate_removals(&root.join(dir), files)?;
    }
    let mut removed = 0;
    for (dir, files) in outputs {
        let out = root.join(dir);
        for file in files {
            let path = out.join(file);
            match fs::remove_file(&path) {
                Ok(()) => {
                    removed += 1;
                    remove_empty_parents(&path, &out);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let _ = fs::remove_dir(out);
    }
    Ok(removed)
}
