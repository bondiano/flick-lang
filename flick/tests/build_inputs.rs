mod common;

use std::fs;
use std::path::Path;

use common::{flick_in, project};
use serde_json::Value;

fn inventory(dir: &Path, file: &str) -> Value {
    let out = flick_in(dir, &["build", file, "--inputs"]);
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("input inventory JSON")
}

#[test]
fn inventory_discovers_project_roots_and_tracks_inputs_without_building() {
    let config = "schema-version = 1\n[project]\nkind = \"lib\"\nsource-dir = \"code\"\ntest-dir = \"spec\"\n";
    let dir = project(
        "build_inputs_project",
        &[
            ("flick.toml", config),
            ("package.json", r#"{"dependencies":{"missing":"1.0.0"}}"#),
            (
                "code/Nested/Widget.fl",
                "import Shared\n\nanswer : Number\nlet answer = Shared.answer\n",
            ),
            (
                "code/Shared.fl",
                "module Shared exposing (answer)\nanswer : Number\nlet answer = 1\n",
            ),
        ],
    );
    let cwd = dir.join("code/Nested");
    let first = inventory(&cwd, "Widget.fl");
    for file in [
        "flick.toml",
        "code/flick.toml",
        "code/Nested/flick.toml",
        "code/Nested/Widget.fl",
        "code/Shared.fl",
        "package.json",
        "pnpm-lock.yaml",
        "node_modules/missing/package.json",
    ] {
        assert!(
            first["files"]
                .as_array()
                .expect("files")
                .contains(&serde_json::json!(dir.join(file))),
            "{file}: {first}",
        );
    }
    assert_eq!(
        first["roots"],
        serde_json::json!([dir.join("code"), dir.join("spec")])
    );
    assert_eq!(first["modules"][0]["name"], "Nested.Widget");
    assert_eq!(
        first["modules"][0]["path"],
        serde_json::json!(cwd.join("Widget.fl"))
    );
    assert_eq!(inventory(&cwd, "Widget.fl")["key"], first["key"]);
    fs::write(
        dir.join("code/Shared.fl"),
        "module Shared exposing (answer)\nanswer : Number\nlet answer = 2\n",
    )
    .expect("edit source");
    let edited = inventory(&cwd, "Widget.fl");
    assert_ne!(edited["key"], first["key"]);
    fs::write(
        dir.join("flick.toml"),
        format!("{config}\n[build]\nlabels = \"all\"\n"),
    )
    .expect("edit config");
    let configured = inventory(&cwd, "Widget.fl");
    assert_ne!(configured["key"], edited["key"]);
    fs::write(
        dir.join("code/Added.fl"),
        "module Added exposing (answer)\nanswer : Number\nlet answer = 3\n",
    )
    .expect("add source");
    assert_ne!(inventory(&cwd, "Widget.fl")["key"], configured["key"]);
    fs::remove_file(dir.join("code/Added.fl")).expect("remove source");
    assert_eq!(inventory(&cwd, "Widget.fl")["key"], configured["key"]);
    assert!(!dir.join("dist").exists(), "inventory emits no modules");
    assert!(
        !dir.join(".flick").exists(),
        "inventory writes no build cache"
    );
    let conflict = flick_in(&cwd, &["build", "Widget.fl", "--inputs", "--watch"]);
    assert_eq!(conflict.status.code(), Some(2), "{conflict:?}");
}

#[test]
fn standalone_inventory_uses_the_compilers_header_based_import_root() {
    let dir = project(
        "build_inputs_standalone",
        &[
            (
                "App/Main.fl",
                "// the filename is not the module name\nmodule App.Main exposing (answer)\nimport Shared\nanswer : Number\nlet answer = Shared.answer\n",
            ),
            (
                "Shared.fl",
                "module Shared exposing (answer)\nanswer : Number\nlet answer = 42\n",
            ),
        ],
    );
    let inputs = inventory(&dir.join("App"), "Main.fl");
    assert_eq!(inputs["roots"], serde_json::json!([dir]));
    assert_eq!(inputs["modules"][0]["name"], "App.Main");
    assert!(
        inputs["files"]
            .as_array()
            .expect("files")
            .contains(&serde_json::json!(dir.join("Shared.fl")))
    );
}
