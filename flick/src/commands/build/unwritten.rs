//! The files an earlier build wrote that this build does not write: in a Go
//! module, `go build` would compile them with the rest of their package; in
//! the output of files built alone, which has no manifest, they would stay.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path};

use crate::artifact::{Artifact, validate_ancestors};
use flick_core::build::go::FILES;

use crate::cache::remove_empty_parents;

/// Removes from `out_dir` what the lists on disk name and
/// `artifacts` no longer has. Only a listed path inside the module of its
/// list goes: a file nobody listed, or a list edited to name others, keeps
/// every file it does not own.
pub(super) fn remove(out_dir: &Path, artifacts: &[Artifact]) -> io::Result<()> {
    let written: HashSet<&str> = artifacts.iter().map(|a| a.path.as_str()).collect();
    let lists = artifacts
        .iter()
        .map(|a| Path::new(&a.path))
        .filter(|path| path.file_name().is_some_and(|name| name == FILES));
    for list in lists {
        let Ok(listed) = fs::read_to_string(out_dir.join(list)) else {
            continue;
        };
        let module = list.parent().unwrap_or(Path::new(""));
        for line in listed.lines().filter(|line| !written.contains(line)) {
            let path = Path::new(line);
            let inside = path.starts_with(module)
                && (path.components()).all(|c| matches!(c, Component::Normal(_)));
            if !inside || validate_ancestors(out_dir, path).is_err() {
                continue;
            }
            let path = out_dir.join(path);
            match fs::remove_file(&path) {
                Ok(()) => remove_empty_parents(&path, out_dir),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use crate::artifact::Artifact;

    use super::remove;

    fn artifact(path: &str, contents: &str) -> Artifact {
        Artifact::source(path.to_owned(), contents.to_owned())
    }

    /// Only what the old list of a module names, inside that module, goes.
    #[test]
    fn removes_only_the_listed_files_of_the_module() {
        let out = env::temp_dir().join(format!("flick-unwritten-{}", process::id()));
        let _ = fs::remove_dir_all(&out);
        let files = [
            "go/main/m_A.go",
            "go/main/m_B.go",
            "go/main/mine.go",
            "go/other/m_C.go",
            "kept.txt",
        ];
        for file in files {
            let path = out.join(file);
            fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
            fs::write(path, "").expect("write");
        }
        let old =
            "go/main/m_A.go\ngo/main/m_B.go\ngo/other/m_C.go\n../escape\ngo/main/../../kept.txt\n";
        fs::write(out.join("go/main/.flick-files"), old).expect("list");
        let now = [
            artifact("go/main/m_A.go", ""),
            artifact("go/main/.flick-files", "go/main/m_A.go\n"),
        ];
        remove(&out, &now).expect("removes");
        let left: Vec<&str> = (files.iter().copied())
            .filter(|f| out.join(f).exists())
            .collect();
        assert_eq!(
            left,
            [
                "go/main/m_A.go",
                "go/main/mine.go",
                "go/other/m_C.go",
                "kept.txt"
            ]
        );
        let _ = fs::remove_dir_all(&out);
    }

    #[cfg(unix)]
    #[test]
    fn stale_lists_cannot_delete_through_a_symlink_outside_the_output() {
        use std::os::unix::fs::symlink;

        let root = env::temp_dir().join(format!("flick-unwritten-symlink-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let out = root.join("output");
        let outside = root.join("outside");
        fs::create_dir_all(out.join("go/main")).expect("output directory");
        fs::create_dir_all(&outside).expect("outside directory");
        fs::write(outside.join("kept.go"), "owned elsewhere").expect("outside file");
        symlink(&outside, out.join("go/main/redirect")).expect("symlink");
        fs::write(
            out.join("go/main/.flick-files"),
            "go/main/redirect/kept.go\n",
        )
        .expect("old list");
        remove(&out, &[artifact("go/main/.flick-files", "")]).expect("remove stale files");
        assert_eq!(
            fs::read(outside.join("kept.go")).expect("outside file remains"),
            b"owned elsewhere"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
