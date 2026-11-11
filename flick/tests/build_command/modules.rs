use crate::common::{flick_in, project};
use std::fs;

#[test]
fn pure_parameterized_modules_with_lexical_prefixes_remain_available_without_effect() {
    let root = project(
        "modules/core_lexical_prefix",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\neffect = \"effect\"\n",
            ),
            (
                "src/App/Factory.fl",
                "module App.Factory exposing (Make)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\noffset : Int\nlet offset = 1\nmodule Make(K : Input) : Output =\n  answer : Int\n  let answer = K.seed + offset\n",
            ),
            (
                "src/App/Argument.fl",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "src/App/Instances.fl",
                "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory.Make(App.Argument)\n",
            ),
        ],
    );
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "{built:?}");
    for module in ["App/Factory", "App/Instances/Chosen"] {
        assert!(
            root.join(format!("dist/_core/{module}.js")).is_file(),
            "{module} must remain available without Effect"
        );
        let text =
            fs::read_to_string(root.join(format!("dist/{module}.iface"))).expect("interface");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON interface");
        assert!(value["abi"]["core"] == "_core/", "{module}: {value}");
    }
}

#[test]
fn cyclic_structure_alias_members_report_a_cycle_without_crashing() {
    let root = project(
        "modules/cyclic_structure_aliases",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            ("src/App/A.fl", "module App.A = App.B\n"),
            ("src/App/B.fl", "module App.B = App.A\n"),
            (
                "src/App/Root.fl",
                "module App.Root exposing (answer)\nimport App.A.Make\nanswer : Int\nlet answer = 42\n",
            ),
        ],
    );
    let checked = flick_in(&root, &["check", "--offline"]);
    assert_eq!(checked.status.code(), Some(1), "{checked:?}");
    assert!(
        String::from_utf8_lossy(&checked.stderr).contains("FLICK-R006"),
        "{checked:?}"
    );
}

#[test]
fn explicit_module_result_keeps_client_server_reachability_diagnostics() {
    let root = project(
        "modules/ascribed_server_boundary",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentries = { web = { module = \"App.Web\", target = \"client\" } }\n",
            ),
            (
                "src/App/Db.fl",
                "module App.Db exposing (query)\nimport Server exposing (Server)\nquery : String ->{Server} String\nlet query key = key\n",
            ),
            (
                "src/App/Page.fl",
                "module App.Page exposing (load)\nimport App.Db\nload : String ->{} String\nlet load key = provide Server.node { App.Db.query key }\n",
            ),
            (
                "src/App/Web.fl",
                "module App.Web : Public exposing (main)\nimport App.Page\ntype module Public =\n  main : Unit ->{} Unit\nmain : Unit ->{} Unit\nlet main () = ignore (App.Page.load \"key\")\n",
            ),
        ],
    );
    let checked = flick_in(&root, &["check", "--offline"]);
    assert_eq!(checked.status.code(), Some(1), "{checked:?}");
    assert!(
        String::from_utf8_lossy(&checked.stderr).contains("FLICK-E009"),
        "{checked:?}"
    );
}

/// A second build of files alone into the same directory removes what the
/// first wrote of a module the program no longer has: its list in the
/// directory names them. Files no build wrote stay.
#[test]
fn a_module_that_drops_out_leaves_no_js_file_behind() {
    let with = "module A exposing (main)\nimport B\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (Int.to_string (B.twice 21))\n";
    let without = "module A exposing (main)\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (Int.to_string 42)\n";
    let b = "module B exposing (twice)\ntwice : Int -> Int\nlet twice x = x * 2\n";
    for emit in ["js", "ts"] {
        let root = project(
            &format!("modules/dropped_{emit}"),
            &[("A.fl", with), ("B.fl", b), ("out/notes.txt", "mine")],
        );
        let build = || {
            flick_in(
                &root,
                &["build", "A.fl", "--emit", emit, "--out-dir", "out"],
            )
        };
        let first = build();
        assert!(first.status.success(), "{first:?}");
        let dropped = ["B.iface", &format!("B.{emit}"), &format!("B.{emit}.map")];
        for file in dropped {
            assert!(root.join("out").join(file).is_file(), "{file}");
        }
        fs::write(root.join("A.fl"), without).expect("write");
        let second = build();
        assert!(second.status.success(), "{second:?}");
        for file in dropped {
            assert!(!root.join("out").join(file).exists(), "{file}");
        }
        assert_eq!(
            fs::read_to_string(root.join("out/notes.txt")).expect("notes"),
            "mine"
        );
    }
}
