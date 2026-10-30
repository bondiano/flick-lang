mod common;

use std::fs;
use std::path::Path;

use common::{flick_in, project};
use serde_json::Value;

fn inventory(dir: &Path, file: &str) -> Value {
    let out = flick_in(dir, &["build", file, "--inputs"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let inventory: Value = serde_json::from_slice(&out.stdout).expect("input inventory JSON");
    for args in [
        &["build", file, "--inputs", "--silent"][..],
        &["build", file, "--inputs", "--diagnostics", "json"][..],
    ] {
        let out = flick_in(dir, args);
        assert!(out.status.success(), "{out:?}");
        assert!(out.stderr.is_empty(), "{out:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).expect("input inventory JSON"),
            inventory,
            "{args:?}"
        );
    }
    inventory
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
                "import Shared\n\nanswer : Float\nlet answer = Shared.answer\n",
            ),
            (
                "code/Shared.fl",
                "module Shared exposing (answer)\nanswer : Float\nlet answer = 1\n",
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
        "module Shared exposing (answer)\nanswer : Float\nlet answer = 2\n",
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
        "module Added exposing (answer)\nanswer : Float\nlet answer = 3\n",
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
                "// the filename is not the module name\nmodule App.Main exposing (answer)\nimport Shared\nanswer : Float\nlet answer = Shared.answer\n",
            ),
            (
                "Shared.fl",
                "module Shared exposing (answer)\nanswer : Float\nlet answer = 42\n",
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

#[test]
fn a_sibling_of_an_external_root_is_part_of_the_cache_key() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_inputs_sibling_cache");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("project dir");
    fs::create_dir_all(root.join("scratch")).expect("scratch dir");
    fs::write(
        root.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\nsource-dir = \"src\"\n",
    )
    .expect("config");
    fs::write(
        root.join("scratch/External.fl"),
        "module External exposing (value)\n\nimport Helper\n\nvalue : Float\nlet value = Helper.n\n",
    )
    .expect("external");
    let helper = root.join("scratch/Helper.fl");
    let set = |n: u8| {
        fs::write(
            &helper,
            format!("module Helper exposing (n)\n\nn : Float\nlet n = {n}{n}{n}\n"),
        )
        .expect("helper");
    };
    let built = || {
        let out = flick_in(&root, &["build", "scratch/External.fl"]);
        let js = fs::read_to_string(root.join("dist/Helper.js")).unwrap_or_default();
        (out.status.success(), js)
    };
    set(1);
    let (ok, js) = built();
    assert!(ok && js.contains("111"), "{js}");
    // a cache hit would hand back the first artifacts
    set(2);
    let (ok, js) = built();
    assert!(ok && js.contains("222"), "{js}");
    fs::remove_file(&helper).expect("delete");
    assert!(!built().0, "a deleted sibling is not a cache hit");
    set(3);
    let (ok, js) = built();
    assert!(ok && js.contains("333"), "{js}");
}
