//! Fetching the files of one package into the package store: a tag of a git
//! repository, or a gzipped tarball whose hash the manifest names. The store
//! keeps them by the hash of the files, so a package is fetched once and
//! never changes there.

use std::fmt::Write;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use flate2::read::GzDecoder;
use flick_core::project::config::Source;
use sha2::{Digest, Sha256};

/// A package in the store.
pub struct Stored {
    /// What pins its source in `flick.lock`: the commit of a git tag.
    pub pin: Option<String>,
    /// `sha256:` and the hash of its files.
    pub checksum: String,
}

/// Fetches `source` into `store`.
pub fn fetch(source: &Source, store: &Path) -> Result<Stored, String> {
    fs::create_dir_all(store).map_err(|e| format!("cannot create `{}`: {e}", store.display()))?;
    let staging = tempdir(store)?;
    let fetched = match source {
        Source::Git { url, tag, .. } => clone(url, tag, &staging).map(Some),
        Source::Url { url, sha256 } => download(url, sha256, &staging).map(|()| None),
        Source::Path(_) => Err("a path dependency is not fetched".to_owned()),
    };
    let stored = fetched.and_then(|pin| {
        let hash = tree_hash(&staging).map_err(|e| format!("cannot read what was fetched: {e}"))?;
        let dir = store.join(&hash);
        if dir.exists() {
            let _ = fs::remove_dir_all(&staging);
        } else {
            fs::rename(&staging, &dir)
                .map_err(|e| format!("cannot move the package into `{}`: {e}", dir.display()))?;
        }
        Ok(Stored {
            pin,
            checksum: format!("sha256:{hash}"),
        })
    });
    if stored.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    stored
}

/// A fresh directory in `store` for a package being fetched.
fn tempdir(store: &Path) -> Result<PathBuf, String> {
    for attempt in 0..100u32 {
        let dir = store.join(format!(".fetch-{}-{attempt}", process::id()));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("cannot create `{}`: {e}", dir.display())),
        }
    }
    Err(format!(
        "cannot create a directory in `{}`",
        store.display()
    ))
}

/// Clones the tag `tag` of `url` into `dir`, without its history; the commit.
fn clone(url: &str, tag: &str, dir: &Path) -> Result<String, String> {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
        }
    };
    let target = dir.to_string_lossy();
    let cloned = [
        "clone", "--quiet", "--depth", "1", "--branch", tag, "--", url, &target,
    ];
    git(&cloned).map_err(|e| format!("cannot fetch tag `{tag}` of `{url}`: {e}"))?;
    let commit = git(&["-C", &target, "rev-parse", "HEAD"])?;
    fs::remove_dir_all(dir.join(".git")).map_err(|e| format!("cannot drop `.git`: {e}"))?;
    Ok(commit)
}

/// Downloads the tarball at `url`, checks it against `sha256` and unpacks it
/// into `dir`, without the one directory it may wrap its files in.
fn download(url: &str, sha256: &str, dir: &Path) -> Result<(), String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("cannot download `{url}`: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot download `{url}`: {e}"))?;
    let actual = hex(&Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(sha256) {
        return Err(format!(
            "`{url}` has sha256 {actual}, and the manifest names {sha256}"
        ));
    }
    tar::Archive::new(GzDecoder::new(bytes.as_slice()))
        .unpack(dir)
        .map_err(|e| format!("cannot unpack `{url}`: {e}"))?;
    unwrap_single_dir(dir).map_err(|e| format!("cannot unpack `{url}`: {e}"))
}

/// Moves the files of the one directory `dir` holds, if that is all it
/// holds, up into `dir`.
fn unwrap_single_dir(dir: &Path) -> io::Result<()> {
    let entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    let [only] = &entries[..] else {
        return Ok(());
    };
    if !only.is_dir() {
        return Ok(());
    }
    let inner = dir.join(".unwrap");
    fs::rename(only, &inner)?;
    for entry in fs::read_dir(&inner)? {
        let entry = entry?;
        fs::rename(entry.path(), dir.join(entry.file_name()))?;
    }
    fs::remove_dir(inner)
}

/// The hash of the files under `dir`, by their paths: the same files give
/// the same hash wherever and whenever they are fetched.
pub fn tree_hash(dir: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        let bytes = fs::read(dir.join(&relative))?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
}

/// The files under `dir`, as `/`-separated paths relative to `root`.
fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect(root, &path, out)?;
        } else {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let parts: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(parts.join("/"));
        }
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}
