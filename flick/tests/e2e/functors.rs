#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn named_functor_result_runs_from_two_importers_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md2_functor_result");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Source)\ntype module Source =\n  value : Int\n",
        ),
        (
            "Argument.fl",
            "module App.Argument exposing (value)\nvalue : Int\nlet value = 41\n",
        ),
        (
            "Make.fl",
            "module App.Make(K : App.Contracts.Source) exposing (answer)\nanswer : Int\nlet answer = K.value + 1\n",
        ),
        (
            "Parent.fl",
            "module App.Parent exposing (Names)\nmodule Names = App.Make(App.Argument)\n",
        ),
        (
            "First.fl",
            "module App.First exposing (answer)\nimport App.Parent.Names exposing (answer)\n",
        ),
        (
            "Second.fl",
            "module App.Second exposing (answer)\nimport App.Parent.Names exposing (answer)\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.First as First\nimport App.Second as Second\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (First.answer + Second.answer))\n",
        ),
    ] {
        fs::write(source.join(name), text).expect("write source");
    }
    let main = source.join("Main.fl");
    for profile in ["debug", "release"] {
        for emit in ["js", "go"] {
            let out = dir.join(format!("out-{emit}-{profile}"));
            let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&main)
                .arg("--out-dir")
                .arg(&out)
                .arg("--emit")
                .arg(emit)
                .arg("--profile")
                .arg(profile)
                .output()
                .expect("build runs");
            assert!(
                build.status.success(),
                "{emit}: {}",
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
                "{emit}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"84\n", "{emit}/{profile}");
        }
    }
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn case_insensitive_set_keeps_its_policy_across_importers_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md2_case_insensitive_set");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Policy)\ntype module Policy =\n  type Key = String\n  class Eq = Prelude.Eq\n  equality : Eq.Dict Key\n  instance Eq Key from equality\n",
        ),
        (
            "CaseInsensitive.fl",
            "module App.CaseInsensitive exposing (Key, Eq(..), equality)\nimport Prelude exposing (Eq(..))\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun a b -> String.to_lower a == String.to_lower b }\n",
        ),
        (
            "Exact.fl",
            "module App.Exact exposing (Key, Eq(..), equality)\nimport Prelude exposing (Eq(..))\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun a b -> a == b }\n",
        ),
        (
            "Make.fl",
            "module App.Make(K : App.Contracts.Policy) exposing (Set, empty, insert, member, size, from_list, to_list)\ntype Set = Set (List K.Key)\nempty : Set\nlet empty = Set []\ninsert : K.Key -> Set -> Set\nlet insert key (Set values) = if List.contains key values then Set values else Set ([key] ++ values)\nmember : K.Key -> Set -> Bool\nlet member key (Set values) = List.contains key values\nsize : Set -> Int\nlet size (Set values) = List.length values\nfrom_list : List K.Key -> Set\nlet from_list values = List.fold_left (fun set key -> insert key set) empty values\nto_list : Set -> List K.Key\nlet to_list (Set values) = values\n",
        ),
        (
            "Collections.fl",
            "module App.Collections exposing (CaseInsensitiveSet, ExactSet)\nmodule CaseInsensitiveSet = App.Make(App.CaseInsensitive)\nmodule ExactSet = App.Make(App.Exact)\n",
        ),
        (
            "First.fl",
            "module App.First exposing (names)\nimport App.Collections.CaseInsensitiveSet as Set\nnames : Set.Set\nlet names = Set.insert \"Alice\" Set.empty\n",
        ),
        (
            "Second.fl",
            "module App.Second exposing (has_name, count, exact_has_name, exact_count, converted_has_name)\nimport App.Collections.CaseInsensitiveSet as Set\nimport App.Collections.ExactSet as ExactSet\nimport App.First as First\nhas_name : Bool\nlet has_name = Set.member \"ALICE\" First.names\ncount : Int\nlet count = Set.size (Set.insert \"alice\" First.names)\nexact_has_name : Bool\nlet exact_has_name = ExactSet.member \"ALICE\" (ExactSet.insert \"Alice\" ExactSet.empty)\nexact_count : Int\nlet exact_count = ExactSet.size (ExactSet.insert \"alice\" (ExactSet.insert \"Alice\" ExactSet.empty))\nconverted_has_name : Bool\nlet converted_has_name = ExactSet.member \"Alice\" (ExactSet.from_list (Set.to_list First.names))\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.Second as Second\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show Second.has_name ++ \" \" ++ show Second.count ++ \" \" ++ show Second.exact_has_name ++ \" \" ++ show Second.exact_count ++ \" \" ++ show Second.converted_has_name)\n",
        ),
    ] {
        fs::write(source.join(name), text).expect("write source");
    }
    let main = source.join("Main.fl");
    for profile in ["debug", "release"] {
        for emit in ["js", "go"] {
            let out = dir.join(format!("out-{emit}-{profile}"));
            let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&main)
                .arg("--out-dir")
                .arg(&out)
                .arg("--emit")
                .arg(emit)
                .arg("--profile")
                .arg(profile)
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
            assert_eq!(output.stdout, b"true 1 false 2 true\n", "{emit}/{profile}");
        }
    }
    case_insensitive_set_rejects_mixed_policy(&source, &dir);
}

fn case_insensitive_set_rejects_mixed_policy(source: &Path, dir: &Path) {
    // The two results have distinct nominal Set types even though both
    // arguments expose Key = String and the same manifest Eq class.
    let mixing = source.join("Mix.fl");
    fs::write(
        &mixing,
        "module App.Mix exposing (mixed)\nimport App.Collections.ExactSet as ExactSet\nimport App.First as First\nmixed : Bool\nlet mixed = ExactSet.member \"Alice\" First.names\n",
    )
    .expect("write invalid consumer");
    let bad = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .arg(&mixing)
        .arg("--out-dir")
        .arg(dir.join("out-mix"))
        .arg("--emit")
        .arg("js")
        .output()
        .expect("invalid build runs");
    assert!(!bad.status.success(), "mixed policies must be rejected");
    let diagnostic = String::from_utf8_lossy(&bad.stderr);
    assert!(
        diagnostic.contains("T003"),
        "expected nominal type mismatch, got: {diagnostic}"
    );
    assert!(
        diagnostic.contains("expected `App.Collections.ExactSet.Set`, found `App.Collections.CaseInsensitiveSet.Set`"),
        "expected both policy identities, got: {diagnostic}"
    );
    assert!(
        diagnostic.contains("Mix.fl:5:37"),
        "expected the rejected argument span, got: {diagnostic}"
    );
    assert!(
        diagnostic.contains(
            "distinct nominal types; use one module result or convert the values explicitly"
        ),
        "expected actionable help, got: {diagnostic}"
    );
}

#[test]
#[ignore = "needs Node and Go; run `just e2e`"]
fn functor_uses_its_argument_dictionary_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md2_functor_dictionary");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Policy)\ntype module Policy =\n  type Key = String\n  class Eq = Prelude.Eq\n  equality : Eq.Dict Key\n  instance Eq Key from equality\n",
        ),
        (
            "Argument.fl",
            "module App.Argument exposing (Key, Eq(..), equality)\nimport Prelude exposing (Eq(..))\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> false }\n",
        ),
        (
            "Make.fl",
            "module App.Make(K : App.Contracts.Policy) exposing (answer)\nanswer : Bool\nlet answer = eq \"A\" \"A\"\n",
        ),
        (
            "HashContracts.fl",
            "module App.HashContracts exposing (Policy)\ntype module Policy =\n  type Key = String\n  hashing : Hash.Dict Key\n  instance Hash Key from hashing\n",
        ),
        (
            "HashArgument.fl",
            "module App.HashArgument exposing (Key, hashing)\ntype alias Key = String\nhashing : Hash.Dict Key\nlet hashing = Hash.dictionary { Eq = Eq.dictionary { eq = fun _ _ -> false }; hash = fun _ -> 0 }\n",
        ),
        (
            "MakeHash.fl",
            "module App.MakeHash(K : App.HashContracts.Policy) exposing (answer, hash_answer)\nanswer : Bool\nlet answer = eq \"A\" \"A\"\nhash_answer : Int\nlet hash_answer = hash \"A\"\n",
        ),
        (
            "Parent.fl",
            "module App.Parent exposing (Chosen, HashChosen)\nmodule Chosen = App.Make(App.Argument)\nmodule HashChosen = App.MakeHash(App.HashArgument)\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.Parent.Chosen exposing (answer)\nimport App.Parent.HashChosen as HashChosen\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (if answer == false && HashChosen.answer == false && HashChosen.hash_answer == 0 then \"ok\" else \"wrong\")\n",
        ),
    ] {
        fs::write(source.join(name), text).expect("write source");
    }
    let main = source.join("Main.fl");
    let argument = fs::read_to_string(source.join("Argument.fl")).expect("argument source");
    assert!(argument.contains("-> false"));
    for emit in ["js", "go"] {
        for (policy, expected) in [("false", "ok\n"), ("true", "wrong\n")] {
            fs::write(
                source.join("Argument.fl"),
                argument.replace("-> false", &format!("-> {policy}")),
            )
            .expect("update dictionary policy");
            let out = dir.join(format!("out-{emit}"));
            let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&main)
                .arg("--out-dir")
                .arg(&out)
                .arg("--emit")
                .arg(emit)
                .arg("--profile")
                .arg("debug")
                .output()
                .expect("build runs");
            assert!(
                build.status.success(),
                "{emit}: {}",
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
                "{emit}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected.as_bytes(), "{emit}/{policy}");
        }
    }
}
#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn higher_order_factory_with_partial_application_runs_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hom_factory");
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
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output exposing (answer)\nanswer : Int\nlet answer = K.seed + 1\n",
        ),
        (
            "Apply.fl",
            "module App.Apply(F : App.Contracts.Factory)(K : App.Contracts.Input) : App.Contracts.Output exposing (answer)\nmodule Built = F(K)\nanswer : Int\nlet answer = Built.answer\n",
        ),
        (
            "Instances.fl",
            "module App.Instances exposing (Chosen)\nmodule Half = App.Apply(App.Make)\nmodule Chosen = App.Instances.Half(App.Argument)\n",
        ),
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
            assert_eq!(output.stdout, b"42\n", "{emit}/{profile}");
        }
    }
}

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
