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
