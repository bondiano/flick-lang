use super::common::{flick_in, project};
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
