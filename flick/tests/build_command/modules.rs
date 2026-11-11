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
