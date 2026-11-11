#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn higher_order_dictionary_scopes_execute_locally_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom_dictionary_scopes");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Policy, Output, Factory, Namespace)\ntype module Policy =\n  type Key = String\n  equality : Eq.Dict Key\n  instance Eq Key from equality\ntype module Output =\n  answer : Bool\ntype module Factory = (K : Policy) -> Output\ntype module Namespace =\n  module Make : Factory\n",
        ),
        (
            "Always.fl",
            "module App.Always exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> true }\n",
        ),
        (
            "Never.fl",
            "module App.Never exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> false }\n",
        ),
        (
            "Make.fl",
            "module App.Make(A : App.Contracts.Policy)(B : App.Contracts.Policy) : App.Contracts.Output\nanswer : Bool\nlet answer = eq \"x\" \"x\"\n",
        ),
        (
            "Outer.fl",
            "module App.Outer(A : App.Contracts.Policy) : App.Contracts.Factory = functor (B : App.Contracts.Policy) ->\n  answer : Bool\n  let answer = eq \"x\" \"x\"\n",
        ),
        (
            "Capture.fl",
            "module App.Capture(A : App.Contracts.Policy) : App.Contracts.Namespace\nprior : Bool\nlet prior = eq \"x\" \"x\"\nmodule Make(B : App.Contracts.Policy) : App.Contracts.Output =\n  answer : Bool\n  let answer = if prior then eq \"x\" \"x\" else true\n",
        ),
        (
            "Instances.fl",
            "module App.Instances exposing (Curried, Nested, Captured, CapturedOther)\nmodule Curried = App.Make(App.Always)(App.Never)\nmodule Nested = App.Outer(App.Always)(App.Never)\nmodule First = App.Capture(App.Always)\nmodule Second = App.Capture(App.Never)\nmodule Captured = First.Make(App.Never)\nmodule CapturedOther = Second.Make(App.Always)\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.Instances.Curried as Curried\nimport App.Instances.Nested as Nested\nimport App.Instances.Captured as Captured\nimport App.Instances.CapturedOther as CapturedOther\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show Curried.answer ++ \" \" ++ show Nested.answer ++ \" \" ++ show Captured.answer ++ \" \" ++ show CapturedOther.answer ++ \" \" ++ show (eq \"x\" \"x\"))\n",
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
                "{emit}/{profile}: {}",
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
                "{emit}/{profile}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                output.stdout, b"false false false true true\n",
                "{emit}/{profile}"
            );
        }
    }
}

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

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn inferred_private_factory_exports_adt_constructors_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom_inferred_constructors");
    let source = dir.join("App");
    write_hom_expression_sources(
        &source,
        "private module App.Factory(K : App.Contracts.Input) exposing (Box(..), box)\ntype Box a = Box a\nbox : Box Int\nlet box = Box K.seed\n",
        "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.Argument)\n",
    );
    fs::write(source.join("Main.fl"), "module App.Main exposing (main)\nimport App.Instances.Chosen as Chosen\nmain : Unit ->{} Unit\nlet main () =\n  let answer = match Chosen.Box 42 with\n    | Chosen.Box n -> n\n  Platform.stdout_line (show answer)\n").expect("main source");
    assert_hom_execution(&source, &dir, "inferred-constructors", b"42\n");
}
