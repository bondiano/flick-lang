//! The stages of a build a wasm plugin takes part in besides blocks: the
//! files it loads, the IR it rewrites and the files it writes.

use std::fs;
use std::path::Path;

use serde_json::Value;

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
            "module App.Main exposing (main)\n\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line \"{text}\"\n"
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
    // and its exports are what the other modules and the host call
    fs::write(root.join("app/src/App/Main.fl"), main("hide the exports")).expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "{stderr}");
    assert!(
        stderr.contains("FLICK-P047") && stderr.contains("their packages or exports"),
        "{stderr}"
    );
    // the same module builds when the plugin leaves both as they were
    fs::write(root.join("app/src/App/Main.fl"), main("keep the exports")).expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(built.status.success(), "{stderr}");
    // replacing an Int constructor operand with a String while retaining its Int annotation
    // is rejected before layout / code generation
    let con_main = |trigger: &str| {
        format!(
            "module App.Main exposing (main)\n\n\
             type Box a = Box a\n\n\
             main : Unit ->{{Host.Console; AppError}} Unit\n\
             let main () =\n  \
               let _ = Box 1\n  \
               Platform.stdout_line \"{trigger}\"\n"
        )
    };
    fs::write(
        root.join("app/src/App/Main.fl"),
        con_main("corrupt int operand"),
    )
    .expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-P047"), "{stderr}");
    // wrong number of constructor type arguments is rejected before layout
    fs::write(
        root.join("app/src/App/Main.fl"),
        con_main("wrong type args"),
    )
    .expect("write");
    let built = flick(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-P047"), "{stderr}");
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_wasm_ir_plugin_rewrite_reaches_a_fixture_backend_with_valid_types() {
    let source = |greeting: &str| {
        format!(
            "module Fixture.TypedIr exposing (main, greeting)\n\n\
             type Box a =\n  \
               | Box a\n\n\
             greeting : Unit -> (Box String, Box Int)\n\
             let greeting () =\n  \
               let msg = \"{greeting}\"\n  \
               (Box msg, Box 1)\n\n\
             main : Unit -> Unit\n\
             let main () = ()\n"
        )
    };
    let root = app("plugins-ir-fixture-backend", "");
    fs::remove_file(root.join("app/src/App/Main.fl")).expect("remove default main");
    fs::create_dir_all(root.join("app/src/Fixture")).expect("fixture dir");
    fs::write(root.join("app/src/Fixture/TypedIr.fl"), source("hello")).expect("write module");
    let rewriting = LIB.replace(
        "plugin = \"pairs.wasm\"\n",
        "plugin = \"pairs.wasm\"\nhooks = [\"ir\"]\n",
    );
    fs::write(root.join("lib/flick.toml"), rewriting).expect("write lib manifest");

    let backend_component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let backend_bytes =
        fs::read(&backend_component).expect("just wasm-plugins builds flick_test_backend.wasm");
    fs::write(root.join("app/backend.wasm"), &backend_bytes).expect("copy backend");
    let sha = flick_backend::sha256(&backend_bytes);
    let app_manifest = format!(
        "schema-version = 1\n\n\
         [project]\n\
         kind = \"app\"\n\
         entry = \"Fixture.TypedIr\"\n\n\
         [build.backend]\n\
         id = \"test/literal\"\n\
         version = \"1.0.0\"\n\
         sha256 = \"{sha}\"\n\
         component = \"backend.wasm\"\n\n\
         [dependencies]\n\
         pairs = {{ path = \"../lib\" }}\n"
    );
    fs::write(root.join("app/flick.toml"), app_manifest).expect("write app manifest");

    for profile in ["debug", "release"] {
        let built = flick(&root, &["build", "--profile", profile, "--offline"]);
        let stderr = String::from_utf8_lossy(&built.stderr);
        assert!(built.status.success(), "{profile}: {stderr}");
        assert!(
            stderr.contains("FLICK-P048"),
            "{profile}: expected ir-hook warning FLICK-P048 in {stderr}"
        );

        let report_text = fs::read_to_string(root.join("app/dist/typed-ir-report.json"))
            .expect("typed-ir-report.json");
        let report: Value = serde_json::from_str(&report_text).expect("valid report json");
        assert_eq!(report["ir_version"], "0.16.0");
        assert_eq!(report["optimized"], profile == "release");

        let module = &report["modules"]["Fixture.TypedIr"];
        let box_decl = &module["types"]["Box"];
        assert_eq!(box_decl["params"], serde_json::json!(["a"]));
        assert_eq!(
            box_decl["constructors"]["Box"]["fields"],
            serde_json::json!(["a"])
        );

        let greeting = &module["values"]["greeting"];
        assert_eq!(
            greeting["ty"],
            "Prelude.Unit -> (Fixture.TypedIr.Box Prelude.String, Fixture.TypedIr.Box Prelude.Int)"
        );
        let strings = greeting["strings"].as_array().expect("strings");
        assert!(
            strings
                .iter()
                .any(|s| s["value"] == "HELLO" && s["ty"] == "Prelude.String"),
            "{profile}: expected rewritten typed \"HELLO\" in {greeting}"
        );
        assert!(
            !strings.iter().any(|s| s["value"] == "hello"),
            "{profile}: original \"hello\" should have been rewritten by ir hook in {greeting}"
        );
        let con_uses = greeting["con_uses"].as_array().expect("con_uses");
        assert!(
            con_uses.iter().any(|c| {
                c["name"] == "Box"
                    && c["type_args"] == serde_json::json!(["Prelude.String"])
                    && c["fields"] == serde_json::json!(["Prelude.String"])
            }),
            "{profile}: missing typed Box String use in {greeting}"
        );
        assert!(
            con_uses.iter().any(|c| {
                c["name"] == "Box"
                    && c["type_args"] == serde_json::json!(["Prelude.Int"])
                    && c["fields"] == serde_json::json!(["Prelude.Int"])
            }),
            "{profile}: missing typed Box Int use in {greeting}"
        );
    }

    // A type-corrupting rewrite from the `ir` hook is rejected before reaching the fixture backend.
    fs::write(
        root.join("app/src/Fixture/TypedIr.fl"),
        source("corrupt int operand"),
    )
    .expect("write corrupt trigger");
    let built = flick(
        &root,
        &[
            "build",
            "--profile",
            "debug",
            "--out-dir",
            "dist-corrupt",
            "--offline",
        ],
    );
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-P047"), "{stderr}");
    assert!(
        !root.join("app/dist-corrupt/typed-ir-report.json").exists(),
        "corrupted IR must be rejected before invoking the fixture backend"
    );
}
