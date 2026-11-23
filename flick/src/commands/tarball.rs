//! The deterministic Flick source tarball of a package: what a dependent
//! fetches by `{ url, sha256 }` and compiles itself. The same files give the
//! same bytes, so the hash a release publishes stays the hash of its tarball.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use flick_core::project::cache;
use flick_core::project::{self, CONFIG_FILE, Project, is_companion};
use sha2::{Digest, Sha256};

#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::fetch;

/// The canonical archive filename for `[package] name` and `version`.
pub fn file_name(name: &str, version: &str) -> String {
    format!(
        "{}-{version}.tgz",
        name.trim_start_matches('@').replace('/', "-")
    )
}

/// Writes `<name>-<version>.tgz` of `project` into `out`: its manifest, its
/// source directory, any declared `[package] plugin` file, and its readme and
/// license under `package/`. Returns the tarball's path and sha256.
pub fn write(
    project: &Project,
    name: &str,
    version: &str,
    out: &Path,
) -> io::Result<(PathBuf, String)> {
    let root = &project.root;
    let canonical_root = fs::canonicalize(root)?;
    let mut files = vec![root.join(CONFIG_FILE)];

    let source_root = project::normalize(&project.source_root());
    let archive_name = file_name(name, version);
    let dest_file = out.join(&archive_name);
    let skip = skipped_paths(project, &source_root, out, &dest_file);
    collect_sources(&source_root, &skip, &mut files)?;

    // a package ships its sources, not the tests beside them
    files.retain(|path| !is_companion(path));

    // include a library's compiled plugin component when declared and present inside the project
    if let Some(plugin) = project
        .config
        .package
        .as_ref()
        .and_then(|p| p.plugin.as_deref())
    {
        let plugin_path = project::normalize(&root.join(plugin));
        if fs::symlink_metadata(&plugin_path).is_ok() {
            files.push(plugin_path);
        }
    }

    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let upper = path
            .file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        if (upper.starts_with("README") || upper.starts_with("LICENSE"))
            && fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() || m.file_type().is_symlink())
            && path.is_file()
        {
            files.push(path);
        }
    }

    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for path in files {
        let canonical = fs::canonicalize(&path)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("`{}` points outside the project root", path.display()),
            ));
        }
        let normalized = project::normalize(&path);
        let relative = normalized.strip_prefix(root).map_err(|_| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("`{}` is outside the project root", path.display()),
            )
        })?;
        let parts: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        entries.push((format!("package/{}", parts.join("/")), normalized));
    }
    entries.sort();
    entries.dedup();

    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (entry_name, path) in &entries {
        let bytes = fs::read(path)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        archive.append_data(&mut header, entry_name, bytes.as_slice())?;
    }
    let bytes = archive.into_inner()?.finish()?;
    fs::create_dir_all(out)?;
    fs::write(&dest_file, &bytes)?;
    Ok((dest_file, fetch::hex(&Sha256::digest(&bytes))))
}

/// Normalized and canonicalized paths skipped while collecting source files.
#[derive(Default)]
struct SkipSet {
    paths: BTreeSet<PathBuf>,
    canonical: BTreeSet<PathBuf>,
}

impl SkipSet {
    fn insert(&mut self, path: &Path) {
        let norm = project::normalize(path);
        if let Ok(canon) = fs::canonicalize(&norm) {
            self.canonical.insert(canon);
        }
        self.paths.insert(norm);
    }

    fn remove(&mut self, path: &Path) {
        let norm = project::normalize(path);
        if let Ok(canon) = fs::canonicalize(&norm) {
            self.canonical.remove(&canon);
        }
        self.paths.remove(&norm);
    }

    fn contains(&self, path: &Path) -> bool {
        let norm = project::normalize(path);
        if self.paths.contains(&norm) {
            return true;
        }
        fs::canonicalize(&norm)
            .ok()
            .is_some_and(|canon| self.canonical.contains(&canon))
    }
}

/// Paths skipped while collecting source files so `source-dir = "."` never
/// packs VCS metadata, Flick caches, build outputs, target package trees, or
/// the destination archive directory/file itself (including across symlinked
/// aliases).
fn skipped_paths(project: &Project, source_root: &Path, out: &Path, dest_file: &Path) -> SkipSet {
    let mut skip = SkipSet::default();
    let root = project::normalize(&project.root);
    skip.insert(&root.join(".git"));
    skip.insert(&root.join(cache::DIR));
    skip.insert(&root.join(&project.config.build.out_dir));
    skip.insert(out);
    #[cfg(feature = "wasm-backends")]
    for dir in backends::package_dirs(project) {
        skip.insert(&root.join(dir));
    }
    // never skip the source root itself (e.g. `source-dir = "."` with `--out-dir .`)
    skip.remove(source_root);
    // still skip the destination tarball itself when `--out-dir` equals `source_root`
    skip.insert(dest_file);
    skip
}

fn collect_sources(dir: &Path, skip: &SkipSet, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    let mut paths: Vec<PathBuf> = entries
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .map(|e| e.path())
        .collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Skip VCS metadata (including `.git` regular files in worktrees/submodules),
        // `.flick`, and skipped target/output paths before inspecting symlink targets.
        if name == ".git" || name == cache::DIR || skip.contains(&path) {
            continue;
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_dir() {
            collect_sources(&path, skip, out)?;
        } else if meta.file_type().is_file() {
            out.push(path);
        } else if meta.file_type().is_symlink() {
            // refuse symlinks to directories (avoid cycles/escapes); file symlinks
            // are checked against `canonical_root` above
            if path.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "symlink directory `{}` cannot be packed into a source tarball",
                        path.display()
                    ),
                ));
            }
            out.push(path);
        }
    }
    Ok(())
}
