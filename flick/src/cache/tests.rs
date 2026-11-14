use std::{env, fs, process};

use flick_core::project::{self, discover};
use flick_core::source::SourceMap;

use super::{input_files, read_unchanged};
use crate::sources::FsHost;

#[test]
fn binary_artifacts_survive_the_build_cache_with_their_executable_kind() {
    use super::{Signed, load, store};
    use crate::artifact::Artifact;
    use flick_backend::ArtifactKind;
    use flick_core::project::cache::Key;

    let root = env::temp_dir().join(format!("flick-cache-binary-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let key = Signed::from(Key::default());
    let files = vec![Artifact {
        path: "program.wasm".into(),
        contents: vec![0, 255, 128, 10],
        kind: ArtifactKind::Executable,
    }];
    store(&root, &key, &files);
    assert_eq!(load(&root, &key).expect("cached binary files"), files);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_component_of_a_plugin_a_library_provides_is_an_input() {
    let root = env::temp_dir().join(format!("flick-cache-plugin-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app/src/App")).expect("dir");
    fs::create_dir_all(root.join("lib/src")).expect("dir");
    fs::write(
        root.join("app/flick.toml"),
        "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\npairs = { path = \"../lib\" }\n",
    )
    .expect("write");
    fs::write(
        root.join("lib/flick.toml"),
        "schema-version = 1\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, &root.join("app"), None)
        .expect("project")
        .expect("found");
    let inputs = input_files(&project, &[]);
    assert!(
        inputs.iter().any(|p| p.ends_with("lib/pairs.wasm")),
        "{inputs:?}"
    );
    // a file of the extension its plugin loads is a module of the project
    fs::write(
        root.join("lib/flick.toml"),
        "schema-version = 1\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nload = [\"pairs\"]\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(root.join("app/src/App/Words.pairs"), "hello = \"hi\"\n").expect("write");
    fs::write(root.join("app/src/App/Notes.txt"), "not a module").expect("write");
    let inputs = input_files(&project, &[]);
    assert!(
        inputs.iter().any(|p| p.ends_with("App/Words.pairs")),
        "{inputs:?}"
    );
    assert!(
        !inputs.iter().any(|p| p.ends_with("App/Notes.txt")),
        "{inputs:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_source_saved_after_it_was_read_keeps_the_build_out_of_the_cache() {
    let root = env::temp_dir().join(format!("flick-cache-race-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/App")).expect("dir");
    fs::write(
        root.join("flick.toml"),
        "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
    )
    .expect("write");
    let helper = root.join("src/App/Helper.fl");
    fs::write(
        &helper,
        "module App.Helper exposing (n)\n\nn : Float\nlet n = 1\n",
    )
    .expect("write");
    fs::write(
        root.join("src/App/Main.fl"),
        "module App.Main exposing (main)\n\nimport App.Helper\n\nmain : Unit -> Unit\nlet main () =\n  let _ = App.Helper.n\n  ()\n",
    )
    .expect("write");
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let project = discover(&host, &mut sources, &root, None)
        .expect("project")
        .expect("found");
    let roots = [root.join("src/App/Main.fl")];
    let (diagnostics, _) = project::compile(&host, &mut sources, Some(&project), &roots);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(sources.files().any(|f| f.path.ends_with("Helper.fl")));
    assert!(read_unchanged(&project, &roots, &host.cwd, &sources));
    // the editor saves the helper after the compiler read it
    fs::write(
        &helper,
        "module App.Helper exposing (n)\n\nn : Float\nlet n = 2\n",
    )
    .expect("write");
    assert!(!read_unchanged(&project, &roots, &host.cwd, &sources));
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn cached_files_cannot_be_current_through_symlinks() {
    use crate::artifact::Artifact;
    use std::os::unix::fs::symlink;

    let root = env::temp_dir().join(format!("flick-cache-redirect-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let out = root.join("output");
    let outside = root.join("outside");
    fs::create_dir_all(&out).expect("output");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(outside.join("same.js"), "same bytes").expect("outside file");
    let files = [Artifact::source("same.js".into(), "same bytes".into())];
    fs::write(out.join("same.js"), "same bytes").expect("regular file");
    assert!(super::artifacts_current(&out, &files));
    fs::remove_file(out.join("same.js")).expect("remove regular file");
    symlink(outside.join("same.js"), out.join("same.js")).expect("leaf symlink");
    assert!(
        !super::artifacts_current(&out, &files),
        "leaf must not satisfy cache"
    );
    symlink(&outside, out.join("redirect")).expect("parent symlink");
    let files = [Artifact::source(
        "redirect/same.js".into(),
        "same bytes".into(),
    )];
    assert!(
        !super::artifacts_current(&out, &files),
        "parent must not satisfy cache"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn recording_preflights_all_stale_paths_before_removing_any_file() {
    use flick_core::project::cache::{MANIFEST, Manifest};
    use std::os::unix::fs::symlink;

    for (case, stale) in [
        ("escape", "../outside/kept.js"),
        ("redirect", "redirect/kept.js"),
    ] {
        let root = env::temp_dir().join(format!("flick-record-{case}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let out = root.join("output");
        let outside = root.join("outside");
        fs::create_dir_all(&out).expect("output");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(out.join("old.js"), "previous build").expect("old file");
        fs::write(outside.join("kept.js"), "owned elsewhere").expect("outside file");
        symlink(&outside, out.join("redirect")).expect("symlink");
        let mut manifest = Manifest::default();
        manifest.record("output", vec!["old.js".into(), stale.into()]);
        let path = root.join(MANIFEST);
        fs::create_dir_all(path.parent().expect("parent")).expect("manifest dir");
        let original = serde_json::to_vec(&manifest).expect("manifest JSON");
        fs::write(&path, &original).expect("manifest");
        assert!(
            super::record(&root, &out, vec![]).is_err(),
            "refuse {stale}"
        );
        assert_eq!(
            fs::read(out.join("old.js")).expect("old remains"),
            b"previous build"
        );
        assert_eq!(
            fs::read(outside.join("kept.js")).expect("outside remains"),
            b"owned elsewhere"
        );
        assert_eq!(fs::read(&path).expect("manifest remains"), original);
        fs::remove_dir_all(root).expect("cleanup");
    }
}

#[cfg(unix)]
#[test]
fn recording_can_unlink_a_stale_leaf_symlink_in_an_external_output_directory() {
    use std::os::unix::fs::symlink;

    let root = env::temp_dir().join(format!("flick-record-external-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let project = root.join("project");
    let out = root.join("output");
    fs::create_dir_all(&out).expect("output");
    fs::create_dir_all(&project).expect("project");
    fs::write(root.join("kept.js"), "owned elsewhere").expect("outside file");
    super::record(&project, &out, vec!["old.js".into()]).expect("initial record");
    symlink(root.join("kept.js"), out.join("old.js")).expect("leaf symlink");
    super::record(&project, &out, vec![]).expect("remove stale link");
    assert!(fs::symlink_metadata(out.join("old.js")).is_err());
    assert_eq!(
        fs::read(root.join("kept.js")).expect("outside remains"),
        b"owned elsewhere"
    );
    fs::remove_dir_all(root).expect("cleanup");
}
