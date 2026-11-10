//! A build's key with the digest of each input behind it, and the digests
//! of the build cached last: what names the input that made the cache miss.

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use flick_core::project::cache::{self, Key};

/// How many differing inputs a miss names.
const NAMED: usize = 8;

/// The key of a build; it reads as the key itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    key: String,
    /// The digest of each labeled input, sorted.
    parts: Vec<(String, String)>,
}

impl From<Key> for Signed {
    fn from(key: Key) -> Self {
        let (key, parts) = key.signed();
        Self { key, parts }
    }
}

impl Deref for Signed {
    type Target = str;

    fn deref(&self) -> &str {
        &self.key
    }
}

/// Beside the cache, whose directory holds one entry per cached build.
pub fn path(root: &Path) -> PathBuf {
    root.join(cache::DIR).join("cache-inputs.json")
}

/// Records the inputs of the build cached under `key`; without the record a
/// later miss only cannot say what changed.
pub fn save(root: &Path, key: &Signed) {
    if let Ok(json) = serde_json::to_string(&key.parts) {
        let _ = fs::write(path(root), json);
    }
}

/// Why nothing is cached under `key`: the inputs that differ from those of
/// the build cached last, their paths relative to `root`.
pub fn missed(root: &Path, key: &Signed) -> String {
    let saved = fs::read_to_string(path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<(String, String)>>(&text).ok());
    match saved {
        Some(saved) => differing(&saved, &key.parts, &format!("{}/", root.display())),
        None => "no earlier build is recorded".to_owned(),
    }
}

/// The labels whose digest is in only one of `saved` and `current`.
fn differing(saved: &[(String, String)], current: &[(String, String)], root: &str) -> String {
    let only = |these: &'_ [(String, String)], those: &'_ [(String, String)]| {
        let labels: Vec<String> = (these.iter())
            .filter(|part| !those.contains(part))
            .map(|(label, _)| format!("`{}`", label.replace(root, "")))
            .collect();
        labels
    };
    let mut labels = only(saved, current);
    labels.extend(only(current, saved));
    labels.sort_unstable();
    labels.dedup();
    if labels.is_empty() {
        return "the inputs are those of the last cached build, whose entry is gone".to_owned();
    }
    let more = labels.len().saturating_sub(NAMED);
    labels.truncate(NAMED);
    let rest = if more == 0 {
        String::new()
    } else {
        format!(" and {more} more")
    };
    format!(
        "the last cached build differs in {}{rest}",
        labels.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::differing;

    fn parts(parts: &[(&str, &str)]) -> Vec<(String, String)> {
        (parts.iter())
            .map(|(label, digest)| ((*label).to_owned(), (*digest).to_owned()))
            .collect()
    }

    #[test]
    fn a_miss_names_changed_added_and_removed_inputs() {
        let saved = parts(&[
            ("config", "1"),
            ("input /p/src/A.fl", "2"),
            ("input /p/src/B.fl", "3"),
        ]);
        let current = parts(&[
            ("config", "1"),
            ("input /p/src/A.fl", "9"),
            ("input /p/src/C.fl", "4"),
        ]);
        assert_eq!(
            differing(&saved, &current, "/p/"),
            "the last cached build differs in `input src/A.fl`, `input src/B.fl`, `input src/C.fl`"
        );
        assert_eq!(
            differing(&saved, &saved, "/p/"),
            "the inputs are those of the last cached build, whose entry is gone"
        );
    }
}
