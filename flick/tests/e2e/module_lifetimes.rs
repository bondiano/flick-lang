#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn higher_order_reference_fixture_runs_on_js_and_go() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../flick-model/tests/modules/App/Hom/Main.fl");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom_reference_fixture");
    for profile in ["debug", "release"] {
        for emit in ["js", "go"] {
            let out = dir.join(format!("out-{emit}-{profile}"));
            let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&file)
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
                "{emit}/{profile}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let output = if emit == "js" {
                Command::new("node")
                    .arg(out.join(runner_path(false)))
                    .output()
            } else {
                Command::new(out.join(executable(MAIN_ENTRY))).output()
            }
            .expect("program runs");
            assert!(output.status.success(), "{emit}/{profile}: {output:?}");
            assert_eq!(
                output.stdout, b"42 false false false true\n",
                "{emit}/{profile}"
            );
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn higher_order_repository_layers_acquire_and_release_on_js_and_go() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/hom-repository");
    let main = example.join("src/App/Main.fl");
    let expected =
        fs::read(example.join("expected.stdout")).expect("derived lifecycle expectations");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom-repository");
    for profile in ["debug", "release"] {
        for emit in ["js", "go"] {
            let out = dir.join(format!("{emit}-{profile}"));
            let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&main)
                .arg("--out-dir")
                .arg(&out)
                .args(["--emit", emit, "--profile", profile, "--offline"])
                .output()
                .expect("build runs");
            assert!(
                build.status.success(),
                "{emit}/{profile}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let output = if emit == "js" {
                Command::new("node")
                    .arg(out.join(runner_path(false)))
                    .output()
                    .expect("Node runs")
            } else {
                Command::new(out.join(executable(MAIN_ENTRY)))
                    .output()
                    .expect("Go executable runs")
            };
            assert!(
                output.status.success(),
                "{emit}/{profile}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected, "{emit}/{profile}");
            assert!(
                output.stderr.is_empty(),
                "{emit}/{profile}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    repository_logging_requirement_is_public(&example, &dir);
}

fn repository_logging_requirement_is_public(example: &Path, dir: &Path) {
    let source = dir.join("negative/App");
    fs::create_dir_all(&source).expect("negative source directory");
    for entry in fs::read_dir(example.join("src/App")).expect("example sources") {
        let entry = entry.expect("source entry");
        fs::copy(entry.path(), source.join(entry.file_name())).expect("copy fixture source");
    }
    let bad = source.join("Bad.fl");
    fs::write(&bad, "module App.Bad exposing (run)\nimport App.Driver as Driver\nimport App.Instances.Logged as Logged\nrun : Unit ->{Driver.Db; Driver.Boot} String\nlet run () = provide Logged.layer { Logged.get \"x\" }\n").expect("negative fixture");
    let checked = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("check")
        .arg(&bad)
        .args(["--offline", "--diagnostics", "json"])
        .output()
        .expect("check runs");
    assert!(
        !checked.status.success(),
        "the operation must still require Log"
    );
    let diagnostics: Vec<serde_json::Value> = String::from_utf8_lossy(&checked.stderr)
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON diagnostic"))
        .collect();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["code"], "FLICK-E002");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .expect("message")
            .contains("does not allow service `Log`")
    );
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn lexical_prefix_values_initialize_before_their_results_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom_lexical_initialization");
    let source = dir.join("App");
    write_hom_expression_sources(
        &source,
        "module App.Factory exposing (Make, MakeMore, answer)\nplus : Int -> Int -> Int\nlet plus a b = a + b\noffset : Int\nlet offset = plus 0 1\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = plus offset K.seed\nmodule Example = Make(App.Argument)\nincrement : Int\nlet increment = Example.answer - 40\nmodule MakeMore(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = plus (plus offset increment) K.seed\nanswer : Int\nlet answer = Example.answer\n",
        "module App.Instances exposing (Chosen, More)\nmodule Chosen = App.Factory.Make(App.Argument)\nmodule More = App.Factory.MakeMore(App.Argument)\n",
    );
    fs::write(source.join("Main.fl"), "module App.Main exposing (main)\nimport App.Factory as Factory\nimport App.Instances.Chosen as Chosen\nimport App.Instances.More as More\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show Factory.answer ++ \" \" ++ show Chosen.answer ++ \" \" ++ show More.answer)\n").expect("main source");
    assert_hom_execution(&source, &dir, "lexical-prefix", b"42 42 44\n");
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn captured_layer_keeps_one_memoized_identity_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom_captured_layer_identity");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Support",
            "module App.Support exposing (Acquire, Value)\nservice Acquire =\n  next : Unit ->{} Int\nservice Value =\n  get : Unit ->{} Int\n",
        ),
        (
            "Contracts",
            "module App.Contracts exposing (Input, Output)\nimport App.Support as S\ntype module Input =\n  seed : Int\ntype module Output =\n  layer : Layer {S.Acquire} none {S.Value}\n",
        ),
        (
            "Argument",
            "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
        ),
        (
            "Factory",
            "module App.Factory exposing (Make, own_layer)\nimport App.Support as S\nshared : Layer {S.Acquire} none {S.Value}\nlet shared = Layer.from_build_memo (fun _ ->\n  let value = S.Acquire.next ()\n  Context.make S.Value { get = fun () -> value }\n)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  layer : Layer {S.Acquire} none {S.Value}\n  let layer = shared\nmodule Example = Make(App.Argument)\nown_layer : Layer {S.Acquire} none {S.Value}\nlet own_layer = Example.layer\n",
        ),
        (
            "Instances",
            "module App.Instances exposing (A, B)\nmodule A = App.Factory.Make(App.Argument)\nmodule B = App.Factory.Make(App.Argument)\n",
        ),
        (
            "Main",
            "module App.Main exposing (main)\nimport App.Support as S\nimport App.Factory as Factory\nimport App.Instances.A as A\nimport App.Instances.B as B\nmain : Unit ->{} Unit\nlet main () =\n  let count = Ref.make 0\n  let acquire = Layer.succeed S.Acquire { next = fun () -> Ref.modify count (fun n -> (n + 1, n + 1)) }\n  let before = Ref.get count\n  let layers = Layer.override (Layer.override A.layer B.layer) Factory.own_layer\n  let value = provide acquire { provide layers { S.Value.get () } }\n  Platform.stdout_line (show before ++ \" \" ++ show (Ref.get count) ++ \" \" ++ show value)\n",
        ),
    ] {
        fs::write(source.join(format!("{name}.fl")), text).expect("source");
    }
    assert_hom_execution(&source, &dir, "captured-layer", b"0 1 1\n");
}
