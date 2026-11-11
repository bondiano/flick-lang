//! The checked and lowered modules of source packages, saved in the user's
//! cache directory: a check or build takes those of a library whose
//! sources, dependencies, compiler and project configuration are unchanged
//! instead of checking the library again.

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Diagnostic;
use flick_core::project::{
    self, CachedModule, Compilation, Fresh, Host, ModuleCache, PackageCache, Project, store,
};
use flick_core::source::SourceMap;
use sha2::{Digest, Sha256};

use crate::cache::files_under;
use crate::{builtins, fetch, saved, toolchain};

const PREFIX: &str = "packages-";

/// `run` with the modules of packages saved so far, if any, saving the
/// ones it gives back.
pub fn with_saved<R>(run: impl FnOnce(Option<&PackageCache>) -> (R, Vec<Fresh>)) -> R {
    let Some(saved) = Saved::new() else {
        return run(None).0;
    };
    let (result, fresh) = saved.with(|cache| run(Some(cache)));
    saved.save(fresh);
    result
}

/// Compiles what `roots` reach, with the saved modules of packages, and
/// saves those of the packages it checked.
pub fn compile(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    rt: bool,
    kept: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Compilation) {
    with_saved(|saved| {
        let (diagnostics, compilation, fresh) =
            project::compile_with(host, sources, project, roots, saved, rt, kept);
        ((diagnostics, compilation), fresh)
    })
}

/// Where this compiler keeps the modules of packages, and what keys them
/// besides each package.
struct Saved {
    dir: PathBuf,
    /// The start of this binary's file names.
    binary: String,
    salt: String,
}

impl Saved {
    /// `None` without a cache directory or a known binary: nothing is saved.
    fn new() -> Option<Saved> {
        let fingerprint = toolchain::fingerprint()?;
        Some(Saved {
            dir: store::cache_dir()?.join("packages"),
            binary: format!("{PREFIX}{fingerprint}-"),
            salt: format!("{fingerprint} {}", builtins::implicit_hash()),
        })
    }

    /// `f` with the modules saved so far.
    fn with<R>(&self, f: impl FnOnce(&PackageCache) -> R) -> R {
        let saved = |key: &str| -> Option<Vec<CachedModule>> {
            let bytes = fs::read(self.path(key)).ok()?;
            serde_json::from_slice(&bytes).ok()
        };
        f(&PackageCache {
            salt: &self.salt,
            identity: &identity,
            saved: &saved,
        })
    }

    /// Saves `fresh`. Any failure only means the packages are checked again.
    fn save(&self, fresh: Vec<Fresh>) {
        if fresh.is_empty() {
            return;
        }
        // other binaries' files are stale once this one saves
        saved::evict_others(&self.dir, PREFIX, &self.binary);
        for Fresh { key, modules } in fresh {
            let Ok(json) = serde_json::to_vec(&modules) else {
                continue;
            };
            let _ = saved::write(&self.path(&key), &json);
        }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}{key}.json", self.binary))
    }
}

/// A fetched package by the checksum `flick.lock` pins, which names its
/// directory in the package store, where it never changes; a path package
/// by the names and texts of the modules under its source directory `dir`.
fn identity(dir: &Path, checksum: Option<&str>) -> Option<String> {
    if let Some(checksum) = checksum {
        return Some(checksum.to_owned());
    }
    let mut files = Vec::new();
    files_under(dir, &mut files);
    let mut hasher = Sha256::new();
    for file in files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "fl"))
    {
        hasher.update(file.strip_prefix(dir).ok()?.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(file).ok()?);
        hasher.update([0]);
    }
    Some(fetch::hex(&hasher.finalize()))
}
