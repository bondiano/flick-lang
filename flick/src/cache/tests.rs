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
