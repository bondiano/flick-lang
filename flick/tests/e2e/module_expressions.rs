#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn anonymous_nested_and_returned_functors_execute_on_js_and_go() {
    let cases = [
        (
            "returned",
            "module App.Factory(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
            "module App.Instances exposing (Chosen)\nmodule Selected = App.Factory(App.Make)\nmodule Chosen = App.Instances.Selected(App.Argument)\n",
        ),
        (
            "anonymous",
            "module App.Factory(F : App.Contracts.Factory)(K : App.Contracts.Input) : App.Contracts.Output\nmodule Built = F(K)\nanswer : Int\nlet answer = Built.answer\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory((functor (K : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + 1\n))(App.Argument)\n",
        ),
        (
            "nested",
            "module App.Factory exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory.Make(App.Argument)\n",
        ),
        (
            "returned_anonymous",
            "module App.Factory(F : App.Contracts.Factory) : App.Contracts.Factory = functor (K : App.Contracts.Input) ->\n  module Built = F(K)\n  answer : Int\n  let answer = Built.answer\n",
            "module App.Instances exposing (Chosen)\nmodule Selected = App.Factory(App.Make)\nmodule Chosen = App.Instances.Selected(App.Argument)\n",
        ),
    ];
    for (case, factory, instances) in cases {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hom_expression_{case}"));
        let source = dir.join("App");
        fs::create_dir_all(&source).expect("source directory");
        for (name, text) in [
            (
                "Contracts.fl",
                "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n",
            ),
            (
                "Argument.fl",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Make.fl",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
            ),
            ("Factory.fl", factory),
            ("Instances.fl", instances),
            (
                "Main.fl",
                "module App.Main exposing (main)\nimport App.Instances.Chosen as Chosen\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show Chosen.answer)\n",
            ),
        ] {
            fs::write(source.join(name), text).expect("source");
        }
        for profile in ["debug", "release"] {
            for emit in ["js", "go"] {
                let out = dir.join(format!("out-{emit}-{profile}"));
                let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                    .arg("build")
                    .arg(source.join("Main.fl"))
                    .arg("--out-dir")
                    .arg(&out)
                    .arg("--emit")
                    .arg(emit)
                    .arg("--profile")
                    .arg(profile)
                    .output()
                    .expect("build");
                assert!(
                    build.status.success(),
                    "{case}/{emit}/{profile}: {}",
                    String::from_utf8_lossy(&build.stderr)
                );
                let output = if emit == "js" {
                    Command::new("node")
                        .arg(out.join(runner_path(false)))
                        .output()
                        .expect("Node")
                } else {
                    Command::new(out.join(executable(MAIN_ENTRY)))
                        .output()
                        .expect("Go executable")
                };
                assert!(
                    output.status.success(),
                    "{case}/{emit}/{profile}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(output.stdout, b"42\n", "{case}/{emit}/{profile}");
            }
        }
    }
}
