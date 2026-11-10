//! Files a later run reads back from a cache directory: written whole or
//! not at all, and removed once nothing reads them. Every failure only
//! means the work they save is done again.

use std::path::{Path, PathBuf};
use std::{fs, io, process};

/// Writes `bytes` to `path` through a file beside it, so no reader sees
/// half of them, making the directory first. A failure leaves nothing
/// behind.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut pending = path.as_os_str().to_owned();
    pending.push(format!(".{}.tmp", process::id()));
    let pending = PathBuf::from(pending);
    fs::write(&pending, bytes)
        .and_then(|()| fs::rename(&pending, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&pending);
        })
}

/// Removes the files of `dir` named `prefix…` that another binary of the
/// compiler wrote: the names of this one's start with `binary`.
pub fn evict_others(dir: &Path, prefix: &str, binary: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) && !name.starts_with(binary) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Removes all but the `keep` files of `dir` written last.
pub fn keep_newest(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let written = entry.metadata().ok()?.modified().ok()?;
            Some((written, entry.path()))
        })
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    for (_, path) in files.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::path::Path;
    use std::time::{Duration, SystemTime};
    use std::{env, process};

    use super::{evict_others, keep_newest, write};

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("the directory")
            .map(|e| {
                e.expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_write_makes_the_directory_and_leaves_one_whole_file() {
        let dir = env::temp_dir().join(format!("flick-saved-write-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("deep").join("a.json");
        write(&path, b"one").expect("written");
        write(&path, b"two").expect("written over");
        assert_eq!(fs::read(&path).expect("the file"), b"two");
        assert_eq!(names(&dir.join("deep")), ["a.json"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_files_of_other_binaries_are_evicted() {
        let dir = env::temp_dir().join(format!("flick-saved-evict-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        for name in ["kind-old-a", "kind-new-a", "other-old-a"] {
            write(&dir.join(name), b"").expect("written");
        }
        evict_others(&dir, "kind-", "kind-new-");
        assert_eq!(names(&dir), ["kind-new-a", "other-old-a"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_files_written_last_are_kept() {
        let dir = env::temp_dir().join(format!("flick-saved-keep-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        let start = SystemTime::now() - Duration::from_secs(100);
        for (age, name) in ["first", "second", "third", "fourth"].iter().enumerate() {
            let path = dir.join(name);
            write(&path, b"").expect("written");
            let written = start + Duration::from_secs(age as u64);
            File::options()
                .write(true)
                .open(&path)
                .and_then(|file| file.set_modified(written))
                .expect("a time of writing");
        }
        keep_newest(&dir, 2);
        assert_eq!(names(&dir), ["fourth", "third"]);
        keep_newest(&dir, 5);
        assert_eq!(names(&dir), ["fourth", "third"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
