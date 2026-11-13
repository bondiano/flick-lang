//! The stages of a build a wasm plugin takes part in besides blocks: the
//! files it loads, the IR it rewrites and the files it writes.

use std::fs;

use super::{LIB, MAIN, app, flick, trace};

#[test]
fn a_file_a_wasm_plugin_loads_is_a_module_the_project_checks() {
    let main = "module App.Main exposing (main, greeting)\n\nimport App.Words\n\ngreeting : String\nlet greeting = App.Words.hello\n\nmain : Unit ->{; AppError} Unit\nlet main () = ()\n";
    let root = app("plugins-load", main);
    let loading = LIB.replace(
        "plugin = \"pairs.wasm\"\n",
        "plugin = \"pairs.wasm\"\nload = [\"pairs\"]\n",
    );
    fs::write(root.join("lib/flick.toml"), loading).expect("write");
    fs::write(root.join("app/src/App/Words.pairs"), "hello = \"hi\"\n").expect("write");
    let checked = flick(&root, &["check"]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(
        trace(&root).iter().any(|l| l.starts_with("load pairs")),
        "{:?}",
        trace(&root)
    );
    fs::write(
        root.join("app/src/App/Words.pairs"),
        "hello = \"hi\"\noops\n",
    )
    .expect("write");
    let checked = flick(&root, &["check"]);
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(
        stderr.contains("pairs/X004") && stderr.contains("Words.pairs:2"),
        "{stderr}"
    );
}

#[test]
fn a_wasm_plugin_rewrites_what_flick_build_writes() {
    let root = app("plugins-emit", MAIN);
    let emitting = LIB.replace(
        "plugin = \"pairs.wasm\"\n",
        "plugin = \"pairs.wasm\"\nhooks = [\"emit\"]\n",
    );
    fs::write(root.join("lib/flick.toml"), emitting).expect("write");
    let built = flick(&root, &["build"]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let main = fs::read_to_string(root.join("app/dist/App/Main.js")).expect("the module");
    assert!(main.starts_with("// pairs\n"), "{main}");
    let map = fs::read_to_string(root.join("app/dist/App/Main.js.map")).expect("its map");
    assert!(map.contains("\"mappings\":\";"), "{map}");
    assert!(
        trace(&root).iter().any(|l| l == "emit pairs"),
        "{:?}",
        trace(&root)
    );
}

#[test]
fn a_wasm_plugin_rewrites_the_ir_a_build_compiles() {
    let main = |text: &str| {
        format!(
            "module App.Main exposing (main)\n\nmain : Unit ->{{; AppError}} Unit\nlet main () = Platform.stdout_line \"{text}\"\n"
        )
    };
    let root = app("plugins-ir", &main("hello"));
    let rewriting = LIB.replace(
        "plugin = \"pairs.wasm\"\n",
        "plugin = \"pairs.wasm\"\nhooks = [\"ir\"]\n",
    );
    fs::write(root.join("lib/flick.toml"), rewriting).expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(built.status.success(), "{stderr}");
    let main_js = fs::read_to_string(root.join("app/dist/App/Main.js")).expect("the module");
    assert!(
        main_js.contains("\"HELLO\"") && !main_js.contains("\"hello\""),
        "{main_js}"
    );
    assert!(stderr.contains("FLICK-P048"), "{stderr}");
    // IR that does not hold together stops the build, naming the plugin
    fs::write(root.join("app/src/App/Main.fl"), main("break the IR")).expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "{stderr}");
    assert!(
        stderr.contains("FLICK-P047") && stderr.contains("`dangling`"),
        "{stderr}"
    );
    // the package of a module names the tags of its constructors
    fs::write(root.join("app/src/App/Main.fl"), main("move the package")).expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "{stderr}");
    assert!(
        stderr.contains("FLICK-P047") && stderr.contains("their packages or exports"),
        "{stderr}"
    );
}
