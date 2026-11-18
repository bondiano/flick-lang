use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io, process};

use flick_backend::ArtifactKind;

use super::fail::{AT, CRASH};
use super::{JOURNAL, commit, recover};
use crate::artifact::Artifact;

type Tree = BTreeMap<String, Vec<u8>>;

fn tree(dir: &Path) -> Tree {
    fn walk(dir: &Path, top: &Path, tree: &mut Tree) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, top, tree);
            } else {
                let name = path.strip_prefix(top).expect("inside");
                let bytes = fs::read(&path).expect("a file");
                tree.insert(name.to_string_lossy().into_owned(), bytes);
            }
        }
    }
    let mut files = Tree::new();
    walk(dir, dir, &mut files);
    files
}

fn files(entries: &[(&str, &str)]) -> Tree {
    (entries.iter())
        .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
        .collect()
}

/// A directory with a previous publication, and the next one.
struct Case {
    root: PathBuf,
    out: PathBuf,
    manifest: PathBuf,
    old: Tree,
    new: Tree,
    writes: Vec<Artifact>,
    removes: Vec<String>,
}

fn case(name: &str) -> Case {
    let root = env::temp_dir().join(format!("flick-publish-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let out = root.join("out");
    let old = files(&[
        ("a.js", "old a"),
        ("b/c.js", "old c"),
        ("gone/stale.js", "stale"),
        ("user.txt", "not the build's"),
    ]);
    for (path, bytes) in &old {
        let path = out.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, bytes).expect("old file");
    }
    let writes = vec![
        Artifact::source("a.js".into(), "new a".into()),
        Artifact::source("b/c.js".into(), "old c".into()),
        Artifact::source("d/e.js".into(), "new e".into()),
    ];
    let new = files(&[
        ("a.js", "new a"),
        ("b/c.js", "old c"),
        ("d/e.js", "new e"),
        ("user.txt", "not the build's"),
    ]);
    Case {
        manifest: root.join("manifest.json"),
        root,
        out,
        old,
        new,
        writes,
        removes: vec!["gone/stale.js".into()],
    }
}

impl Case {
    fn commit(&self) -> io::Result<()> {
        commit(
            &self.out,
            &self.writes,
            &self.removes,
            Some((self.manifest.clone(), b"new manifest".to_vec())),
        )
    }

    fn state(&self) -> &'static str {
        assert!(!self.out.join(JOURNAL).exists(), "the journal is gone");
        let now = tree(&self.out);
        if now == self.old && !self.manifest.exists() {
            "old"
        } else if now == self.new && fs::read(&self.manifest).is_ok_and(|m| m == b"new manifest") {
            "new"
        } else {
            panic!("a partial publication: {now:?}")
        }
    }
}

/// A failure before the commit point undoes every move; one after it,
/// in writing the manifest, leaves a journal the next publication finishes.
#[test]
fn a_failure_at_any_operation_keeps_the_last_complete_publication() {
    let case = case("failure");
    let mut states = Vec::new();
    for n in 0.. {
        AT.set(Some(n));
        let result = case.commit();
        AT.set(None);
        if result.is_ok() {
            assert_eq!(case.state(), "new");
            break;
        }
        if case.out.join(JOURNAL).exists() {
            recover(&case.out).expect("finishes the commit");
            assert_eq!(case.state(), "new", "after a failure at operation {n}");
            states.push("new");
            fs::remove_file(&case.manifest).expect("manifest");
            for (path, bytes) in &case.old {
                let path = case.out.join(path);
                fs::create_dir_all(path.parent().expect("parent")).expect("directory");
                fs::write(path, bytes).expect("old file");
            }
            fs::remove_dir_all(case.out.join("d")).expect("new directory");
        } else {
            assert_eq!(case.state(), "old", "after a failure at operation {n}");
            states.push("old");
        }
    }
    assert!(
        states.iter().filter(|s| **s == "old").count() > 5,
        "{states:?}"
    );
    assert!(
        states.contains(&"new"),
        "the manifest write can fail: {states:?}"
    );
    fs::remove_dir_all(&case.root).expect("cleanup");
}

#[test]
fn a_crash_at_any_operation_recovers_the_old_or_the_new_publication() {
    let mut seen = Vec::new();
    for n in 0.. {
        let case = case(&format!("crash-{n}"));
        AT.set(Some(n));
        CRASH.set(true);
        let result = case.commit();
        AT.set(None);
        CRASH.set(false);
        if result.is_ok() {
            fs::remove_dir_all(&case.root).expect("cleanup");
            break;
        }
        recover(&case.out).expect("recovers");
        seen.push(case.state());
        fs::remove_dir_all(&case.root).expect("cleanup");
    }
    assert!(seen.contains(&"old") && seen.contains(&"new"), "{seen:?}");
}

#[test]
fn a_path_inside_the_journal_is_refused_before_any_change() {
    let case = case("journal");
    let writes = [Artifact::source(format!("{JOURNAL}/x"), "x".into())];
    assert!(commit(&case.out, &writes, &[], None).is_err());
    assert_eq!(tree(&case.out), case.old);
    fs::remove_dir_all(&case.root).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn an_executable_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;

    let case = case("mode");
    let program = Artifact {
        path: "bin/program".into(),
        contents: b"#!/bin/sh\n".to_vec(),
        kind: ArtifactKind::Executable,
    };
    commit(&case.out, &[program], &[], None).expect("published");
    let mode = fs::metadata(case.out.join("bin/program"))
        .expect("program")
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111);
    fs::remove_dir_all(&case.root).expect("cleanup");
}
