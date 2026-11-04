//! What plugins gave in earlier runs, in the user's cache directory: a file
//! per call, named by this binary and the hash of the call's key. A file of
//! another binary is stale once this one saves. Any failure only means the
//! plugin is called again.

use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use std::{fs, process};

use flick_core::project::store;
use sha2::{Digest, Sha256};

use crate::{fetch, toolchain};

const PREFIX: &str = "plugin-";

/// The directory and the start of this binary's file names.
fn location() -> Option<&'static (PathBuf, String)> {
    static LOCATION: OnceLock<Option<(PathBuf, String)>> = OnceLock::new();
    LOCATION
        .get_or_init(|| {
            let dir = store::cache_dir()?.join("plugins");
            Some((dir, format!("{PREFIX}{}-", toolchain::fingerprint()?)))
        })
        .as_ref()
}

/// What a plugin gave for the call `key` in an earlier run.
pub fn cached(key: &[u8]) -> Option<Vec<u8>> {
    let (dir, binary) = location()?;
    read(dir, binary, key)
}

/// Keeps what a plugin gave for the call `key`.
pub fn save(key: &[u8], value: &[u8]) {
    static EVICTED: Once = Once::new();
    let Some((dir, binary)) = location() else {
        return;
    };
    EVICTED.call_once(|| evict(dir, binary));
    write(dir, binary, key, value);
}

fn file(dir: &Path, binary: &str, key: &[u8]) -> PathBuf {
    dir.join(format!("{binary}{}.bin", fetch::hex(&Sha256::digest(key))))
}

fn read(dir: &Path, binary: &str, key: &[u8]) -> Option<Vec<u8>> {
    fs::read(file(dir, binary, key)).ok()
}

fn write(dir: &Path, binary: &str, key: &[u8], value: &[u8]) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = file(dir, binary, key);
    let pending = path.with_extension(format!("{}.tmp", process::id()));
    if fs::write(&pending, value).is_ok() && fs::rename(&pending, &path).is_err() {
        let _ = fs::remove_file(&pending);
    }
}

/// Removes the files of other binaries.
fn evict(dir: &Path, binary: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PREFIX) && !name.starts_with(binary) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::{evict, read, write};

    #[test]
    fn a_saved_result_reads_back_until_another_binary_saves() {
        let dir = env::temp_dir().join(format!("flick-plugin-cache-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        write(&dir, "plugin-old-", b"key", b"old");
        write(&dir, "plugin-new-", b"key", b"value");
        assert_eq!(
            read(&dir, "plugin-new-", b"key").as_deref(),
            Some(b"value".as_slice())
        );
        assert_eq!(read(&dir, "plugin-new-", b"other"), None);
        evict(&dir, "plugin-new-");
        assert_eq!(read(&dir, "plugin-old-", b"key"), None);
        assert_eq!(
            read(&dir, "plugin-new-", b"key").as_deref(),
            Some(b"value".as_slice())
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
