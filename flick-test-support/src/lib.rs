//! What the test suites of the workspace share: the fixtures under a
//! directory and the lines of a list of cases.

use std::fs;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

static ACTIVE_HELPERS: AtomicUsize = AtomicUsize::new(0);

struct HelperPermit;

impl Drop for HelperPermit {
    fn drop(&mut self) {
        ACTIVE_HELPERS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn acquire_helpers(max_helpers: usize, limit: usize) -> Vec<HelperPermit> {
    let mut permits = Vec::new();
    while permits.len() < max_helpers {
        let current = ACTIVE_HELPERS.load(Ordering::Acquire);
        if current >= limit {
            break;
        }
        if ACTIVE_HELPERS
            .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            permits.push(HelperPermit);
        }
    }
    permits
}

/// Maps `f` over `items` across available CPU cores, returning results in the
/// exact input order while bounding process-wide helper threads to avoid
/// oversubscription when multiple test functions run concurrently.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(f).collect();
    }
    let parallelism = thread::available_parallelism().map_or(1, NonZero::get);
    let permits = acquire_helpers(items.len().saturating_sub(1), parallelism.saturating_sub(1));
    if permits.is_empty() {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let work = || {
        let mut local = Vec::new();
        loop {
            let idx = next.fetch_add(1, Ordering::Relaxed);
            let Some(item) = items.get(idx) else {
                break;
            };
            local.push((idx, f(item)));
        }
        local
    };
    let mut indexed: Vec<(usize, R)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..permits.len()).map(|_| scope.spawn(work)).collect();
        let mut all = Vec::with_capacity(items.len());
        all.extend(work());
        for handle in handles {
            all.extend(handle.join().expect("worker panicked"));
        }
        all
    });
    drop(permits);
    indexed.sort_unstable_by_key(|(idx, _)| *idx);
    indexed.into_iter().map(|(_, value)| value).collect()
}

/// The files with the extension `ext` under `dir`, at any depth, sorted;
/// none under a hidden directory, installed packages or build output. A
/// directory that cannot be read has none.
pub fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, ext, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || matches!(name, "node_modules" | "dist" | "target") {
            continue;
        }
        if path.is_dir() {
            walk(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// The entries of a list: each line without its `# why`, blank ones left out.
pub fn entries(text: &str) -> Vec<&str> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{entries, files, par_map};
    use std::path::Path;

    #[test]
    fn par_map_preserves_input_order() {
        let items: Vec<usize> = (0..64).collect();
        assert_eq!(
            par_map(&items, |n| n * 3 + 1),
            items.iter().map(|n| n * 3 + 1).collect::<Vec<_>>()
        );
        assert_eq!(par_map(&[] as &[usize], |n| *n), Vec::<usize>::new());
    }

    #[test]
    fn a_list_drops_its_reasons_and_blank_lines() {
        assert_eq!(
            entries("a/b # why\n\n# only a note\n  go c/d  \n"),
            ["a/b", "go c/d"]
        );
    }

    #[test]
    fn the_fixtures_of_the_repository_are_found_sorted() {
        let run = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
        let found = files(&run, "fl");
        assert!(found.len() > 100, "{}", found.len());
        assert!(found.windows(2).all(|w| w[0] < w[1]));
        assert!(
            found
                .iter()
                .all(|p| p.extension().is_some_and(|e| e == "fl"))
        );
    }
}
