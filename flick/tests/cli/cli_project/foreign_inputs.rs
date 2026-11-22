use super::*;

#[test]
fn a_library_declaration_edit_invalidates_the_published_foreign_bytes() {
    for extension in ["d.ts", "d.mts", "d.cts"] {
        let root = project(
            &format!("cli_foreign_declaration_{extension}"),
            &[
                (
                    "app/flick.toml",
                    "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\nforeign = { path = \"../library\" }\n",
                ),
                (
                    "app/src/Main.fl",
                    "module Main exposing (inc)\nimport Library\ninc : Int ->{} Int\nlet inc n = Library.inc n\n",
                ),
                (
                    "library/flick.toml",
                    "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"foreign-library\"\nversion = \"1.0.0\"\n",
                ),
                (
                    "library/src/Library.fl",
                    "module Library exposing (inc)\ninc : Int ->{} Int\nforeign inc from \"./helper.mjs\" \"inc\"\n",
                ),
                (
                    "library/src/helper.mjs",
                    "export const inc = (n) => (n + 1) | 0;\n",
                ),
            ],
        );
        let name = format!("helper.{extension}");
        let source = root.join("library/src").join(&name);
        let before = "export declare const marker: 'before';\n";
        let after = "export declare const marker: 'after';\n";
        fs::write(&source, before).expect("declaration");
        let app = root.join("app");
        for _ in 0..2 {
            let result = flick_in(&app, &["build"]);
            assert!(result.status.success(), "{result:?}");
            assert_eq!(
                fs::read_to_string(app.join("dist").join(&name)).expect("shipped declaration"),
                before
            );
            assert_eq!(cached(&app), 1);
        }
        fs::write(source, after).expect("edit only the foreign declaration");
        for _ in 0..2 {
            let result = flick_in(&app, &["build"]);
            assert!(result.status.success(), "{result:?}");
            assert_eq!(
                fs::read_to_string(app.join("dist").join(&name)).expect("updated declaration"),
                after
            );
            assert_eq!(
                cached(&app),
                2,
                "the edit creates a distinct build cache entry"
            );
        }
    }
}

#[test]
fn generated_content_mapper_declarations_and_host_ts_are_not_js_build_inputs() {
    let dir = project(
        "cli_js_foreign_suffixes",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (answer)\n\nanswer : Int\nlet answer = 42\n",
            ),
        ],
    );
    let inputs_of = |dir: &Path| -> serde_json::Value {
        let out = flick_in(dir, &["build", "--inputs", "--color", "never"]);
        assert!(out.status.success(), "{out:?}");
        serde_json::from_slice(&out.stdout).expect("inputs json")
    };

    let before = inputs_of(&dir);

    // Generating `Main.d.fl.ts` alongside `.fl` files and adding host `.ts`/`.js`/`.mts`
    // files that Flick never ships must not change the compiler's input inventory or key.
    let declared = flick_in(&dir, &["content-mapper", "--declarations", "src"]);
    assert!(declared.status.success(), "{declared:?}");
    assert!(dir.join("src/App/Main.d.fl.ts").is_file());
    fs::write(dir.join("src/App/page.ts"), "export const x = 1;\n").expect("write page.ts");
    fs::write(dir.join("src/App/wrapper.js"), "export const y = 2;\n").expect("write wrapper.js");
    fs::write(dir.join("src/App/helper.mts"), "export const z = 3;\n").expect("write helper.mts");

    let after_non_shipped = inputs_of(&dir);
    assert_eq!(
        after_non_shipped["key"], before["key"],
        "generated `.d.fl.ts` and unshipped `.ts`/`.js`/`.mts` files must not change `--inputs` key"
    );
    let listed: Vec<&str> = after_non_shipped["files"]
        .as_array()
        .expect("files array")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    for excluded in ["Main.d.fl.ts", "page.ts", "wrapper.js", "helper.mts"] {
        assert!(
            !listed.iter().any(|p| p.ends_with(excluded)),
            "{excluded} must not be tracked as a JS build input: {listed:?}"
        );
    }

    // Shipped JS foreign suffixes remain tracked.
    for shipped in [
        "helper.mjs",
        "helper.cjs",
        "helper.d.ts",
        "helper.d.mts",
        "helper.d.cts",
    ] {
        fs::write(dir.join("src/App").join(shipped), "export {};\n").expect("write shipped");
    }
    let after_shipped = inputs_of(&dir);
    assert_ne!(after_shipped["key"], before["key"]);
    let shipped_listed: Vec<&str> = after_shipped["files"]
        .as_array()
        .expect("files array")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    for shipped in [
        "helper.mjs",
        "helper.cjs",
        "helper.d.ts",
        "helper.d.mts",
        "helper.d.cts",
    ] {
        assert!(
            shipped_listed.iter().any(|p| p.ends_with(shipped)),
            "{shipped} must be tracked as a JS build input: {shipped_listed:?}"
        );
    }
}
