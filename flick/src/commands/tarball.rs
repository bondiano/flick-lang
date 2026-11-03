//! The source tarball of a library: what a dependent fetches by
//! `{ url, sha256 }` and compiles itself. The same files give the same
//! bytes, so the hash a release publishes stays the hash of its tarball.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use flick_core::project::{CONFIG_FILE, Project};
use sha2::{Digest, Sha256};

use crate::cache::files_under;
use crate::fetch;

/// Writes `<name>-<version>.tgz` of the library `project` into `out`: its
/// manifest, its source directory and its readme and license, under
/// `package/`. The tarball's path and sha256.
pub fn write(
    project: &Project,
    name: &str,
    version: &str,
    out: &Path,
) -> io::Result<(PathBuf, String)> {
    let root = &project.root;
    let mut files = vec![root.join(CONFIG_FILE)];
    files_under(&project.source_root(), &mut files);
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let upper = path
            .file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        if path.is_file() && (upper.starts_with("README") || upper.starts_with("LICENSE")) {
            files.push(path);
        }
    }
    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(root).ok()?;
            let parts: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            Some((format!("package/{}", parts.join("/")), path))
        })
        .collect();
    entries.sort();
    entries.dedup();
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, path) in &entries {
        let bytes = fs::read(path)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        archive.append_data(&mut header, name, bytes.as_slice())?;
    }
    let bytes = archive.into_inner()?.finish()?;
    fs::create_dir_all(out)?;
    let file = out.join(format!(
        "{}-{version}.tgz",
        name.trim_start_matches('@').replace('/', "-")
    ));
    fs::write(&file, &bytes)?;
    Ok((file, fetch::hex(&Sha256::digest(&bytes))))
}
