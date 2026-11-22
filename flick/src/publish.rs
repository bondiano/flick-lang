//! One publication into an output directory: all of its writes and
//! removals take effect, or none does. Files are staged in a journal inside
//! the directory, so every move is a rename on one filesystem; a later
//! publication finishes or undoes one that a crash interrupted.
//!
//! This guards the last complete publication against failures and crashes,
//! not against a reader that looks at the directory during the commit.

use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{io, process};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use flick_core::project::store::cache_dir;

use crate::artifact::Artifact;
use crate::cache::remove_empty_parents;
use crate::saved;

/// The journal of a publication into an output directory, inside it.
pub(crate) const JOURNAL: &str = ".flick-publish";

#[derive(Serialize, Deserialize)]
struct Journal {
    writes: Vec<String>,
    removes: Vec<String>,
    /// A file outside the directory, as the project's output manifest,
    /// written once the files are in place.
    manifest: Option<(PathBuf, Vec<u8>)>,
}

/// Publishes `writes` and removes `removes` under `out`, then writes
/// `manifest`. Paths are relative and validated by the caller; a failure
/// before the commit point leaves `out` as it was.
pub(crate) fn commit<'a>(
    out: &Path,
    writes: impl IntoIterator<Item = &'a Artifact>,
    removes: &[String],
    manifest: Option<(PathBuf, Vec<u8>)>,
) -> io::Result<()> {
    let writes: Vec<&Artifact> = writes.into_iter().collect();
    if let Some(path) = (writes.iter().map(|a| &a.path))
        .chain(removes)
        .find(|path| owned(path))
    {
        return Err(io::Error::other(format!(
            "`{path}` is inside `{JOURNAL}`, which the compiler owns"
        )));
    }
    let _lock = lock(out)?;
    let journal = out.join(JOURNAL);
    recover_locked(out, &journal)?;
    let writes: Vec<&Artifact> = (writes.into_iter())
        .filter(|a| !a.current(&out.join(&a.path)))
        .collect();
    let removes: Vec<String> = (removes.iter())
        .filter(|p| fs::symlink_metadata(out.join(p)).is_ok())
        .cloned()
        .collect();
    let record = Journal {
        writes: writes.iter().map(|a| a.path.clone()).collect(),
        removes,
        manifest,
    };
    if record.writes.is_empty() && record.removes.is_empty() {
        return finish(out, &journal, &record);
    }
    let staged = stage(&journal, &writes, &record);
    if let Err(error) = staged {
        if !fail::crashed() {
            let _ = fs::remove_dir_all(&journal);
        }
        return Err(error);
    }
    if let Err(error) = apply(out, &journal, &record) {
        if !fail::crashed() {
            undo(out, &journal, &record)?;
        }
        return Err(error);
    }
    finish(out, &journal, &record)
}

/// Finishes or undoes a publication into `out` that a crash interrupted;
/// the next publication into it does this first.
#[cfg(test)]
pub(crate) fn recover(out: &Path) -> io::Result<()> {
    let _lock = lock(out)?;
    recover_locked(out, &out.join(JOURNAL))
}

fn owned(path: &str) -> bool {
    Path::new(path)
        .components()
        .next()
        .is_some_and(|c| c == Component::Normal(JOURNAL.as_ref()))
}

/// Serializes the publications of processes into one directory, through a
/// lock file in the cache, so the directory gets no file of its own.
fn lock(out: &Path) -> io::Result<Option<File>> {
    let Some(dir) = cache_dir() else {
        return Ok(None);
    };
    let out = fs::canonicalize(out).unwrap_or_else(|_| out.to_path_buf());
    let name = format!(
        "{:x}.lock",
        Sha256::digest(out.as_os_str().as_encoded_bytes())
    );
    let dir = dir.join("publish");
    fs::create_dir_all(&dir)?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(name))?;
    file.lock()?;
    Ok(Some(file))
}

fn stage(journal: &Path, writes: &[&Artifact], record: &Journal) -> io::Result<()> {
    let new = journal.join("new");
    fs::create_dir_all(&new)?;
    fs::create_dir_all(journal.join("old"))?;
    for (i, artifact) in writes.iter().enumerate() {
        let path = new.join(i.to_string());
        fail::point()?;
        fs::write(&path, &artifact.contents)?;
        artifact.restore_permissions(&path)?;
    }
    let mut bytes = Vec::new();
    ciborium::into_writer(record, &mut bytes).map_err(io::Error::other)?;
    fail::point()?;
    saved::write(&journal.join("journal.cbor"), &bytes)
}

/// Removals first, so a write may take the place of a removed file.
fn apply(out: &Path, journal: &Path, record: &Journal) -> io::Result<()> {
    for (i, path) in record.removes.iter().enumerate() {
        fail::point()?;
        fs::rename(out.join(path), journal.join(format!("old/r{i}")))?;
    }
    for (i, path) in record.writes.iter().enumerate() {
        let to = out.join(path);
        if fs::symlink_metadata(&to).is_ok() {
            fail::point()?;
            fs::rename(&to, journal.join(format!("old/w{i}")))?;
        }
        if let Some(dir) = to.parent() {
            fail::point()?;
            fs::create_dir_all(dir)?;
        }
        fail::point()?;
        fs::rename(journal.join(format!("new/{i}")), &to)?;
    }
    fail::point()?;
    File::create(journal.join("committed")).map(drop)
}

/// Puts every earlier file back. A staged file that is no longer in the
/// journal is in its place in `out`.
fn undo(out: &Path, journal: &Path, record: &Journal) -> io::Result<()> {
    for (i, path) in record.writes.iter().enumerate().rev() {
        let to = out.join(path);
        if !journal.join(format!("new/{i}")).exists() {
            match fs::remove_file(&to) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        let old = journal.join(format!("old/w{i}"));
        if fs::symlink_metadata(&old).is_ok() {
            fs::rename(old, &to)?;
        } else {
            remove_empty_parents(&to, out);
        }
    }
    for (i, path) in record.removes.iter().enumerate().rev() {
        let old = journal.join(format!("old/r{i}"));
        if fs::symlink_metadata(&old).is_ok() {
            fs::rename(old, out.join(path))?;
        }
    }
    fs::remove_dir_all(journal)
}

fn finish(out: &Path, journal: &Path, record: &Journal) -> io::Result<()> {
    if let Some((path, bytes)) = &record.manifest {
        fail::point()?;
        saved::write(path, bytes)?;
    }
    for path in &record.removes {
        remove_empty_parents(&out.join(path), out);
    }
    match fs::remove_dir_all(journal) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn recover_locked(out: &Path, journal: &Path) -> io::Result<()> {
    let Ok(bytes) = fs::read(journal.join("journal.cbor")) else {
        // nothing moved before the journal was written: only staged copies remain
        return match fs::remove_dir_all(journal) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    };
    let record: Journal = ciborium::from_reader(bytes.as_slice())
        .map_err(|e| io::Error::other(format!("invalid publication journal: {e}")))?;
    if (record.writes.iter().chain(&record.removes)).any(|p| !relative(p)) {
        return Err(io::Error::other(
            "publication journal names a path outside its directory",
        ));
    }
    if journal.join("committed").exists() {
        finish(out, journal, &record)
    } else {
        undo(out, journal, &record)
    }
}

fn relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// A directory of its own beside an output directory, so that paths
/// relative to it resolve as they do from the output, for the tools that
/// make files the output then gets. It is removed on drop.
#[cfg_attr(not(feature = "wasm-backends"), allow(dead_code))]
pub(crate) struct Beside(PathBuf);

#[cfg_attr(not(feature = "wasm-backends"), allow(dead_code))]
impl Beside {
    pub(crate) fn new(out: &Path) -> io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let (Some(parent), Some(name)) = (out.parent(), out.file_name()) else {
            return Err(io::Error::other(format!(
                "no directory beside `{}` for host tools",
                out.display()
            )));
        };
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = parent.join(format!(
            ".{}.flick-work-{}-{n}",
            name.to_string_lossy(),
            process::id()
        ));
        fs::create_dir_all(parent)?;
        // a fresh name: never a directory another process uses
        fs::create_dir(&dir)?;
        Ok(Self(dir))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Beside {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Injected failures for the tests: the `n`th filesystem operation of a
/// publication fails, and a crash leaves the journal as it was.
mod fail {
    #[cfg(test)]
    use std::cell::Cell;
    use std::io;

    #[cfg(test)]
    thread_local! {
        pub(super) static AT: Cell<Option<usize>> = const { Cell::new(None) };
        pub(super) static CRASH: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn point() -> io::Result<()> {
        if injected() {
            Err(io::Error::other("injected failure"))
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    fn injected() -> bool {
        AT.with(|at| {
            let left = at.get();
            at.set(left.and_then(|n| n.checked_sub(1)));
            left == Some(0)
        })
    }

    #[cfg(not(test))]
    fn injected() -> bool {
        false
    }

    #[cfg(test)]
    pub(super) fn crashed() -> bool {
        CRASH.with(Cell::get)
    }

    #[cfg(not(test))]
    pub(super) fn crashed() -> bool {
        false
    }
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
