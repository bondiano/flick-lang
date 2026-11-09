mod common;

use std::fs;
use std::path::Path;

use common::flick_in;

#[test]
fn build_ui_names_the_renderer_in_place_of_react() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_ui");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\nui = \"./renderer.mjs\"\n",
    )
    .expect("write");
    fs::write(
        dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () =\n  let _ = Ui.text \"hi\"\n  ()\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    let ui = fs::read_to_string(dir.join("dist/_flick/Ui.js")).expect("Ui.js");
    assert!(ui.contains("from \"../../renderer.mjs\""), "{ui}");
    assert!(!ui.contains("@flick-lang/effect-ts/ui"), "{ui}");
}
