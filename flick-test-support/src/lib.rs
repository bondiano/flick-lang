//! What the test suites of the workspace share: the fixtures under a
//! directory and the lines of a list of cases.

use std::fs;
use std::path::{Path, PathBuf};

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
    use super::{entries, files};
    use std::path::Path;

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
