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
            "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : Int\nlet answer = K.value + 1\n\ntype module PublicResult =\n  answer : Int\n",
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
            "module App.Make(K : App.Contracts.Policy) : PublicResult exposing (Set, empty, insert, member, size, from_list, to_list)\ntype Set = Set (List K.Key)\nempty : Set\nlet empty = Set []\ninsert : K.Key -> Set -> Set\nlet insert key (Set values) = if List.contains key values then Set values else Set ([key] ++ values)\nmember : K.Key -> Set -> Bool\nlet member key (Set values) = List.contains key values\nsize : Set -> Int\nlet size (Set values) = List.length values\nfrom_list : List K.Key -> Set\nlet from_list values = List.fold_left (fun set key -> insert key set) empty values\nto_list : Set -> List K.Key\nlet to_list (Set values) = values\n\ntype module PublicResult =\n  type Set\n  empty : Set\n  insert : K.Key -> Set -> Set\n  member : K.Key -> Set -> Bool\n  size : Set -> Int\n  from_list : List K.Key -> Set\n  to_list : Set -> List K.Key\n",
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
            "module App.Make(K : App.Contracts.Policy) : PublicResult exposing (answer)\nanswer : Bool\nlet answer = eq \"A\" \"A\"\n\ntype module PublicResult =\n  answer : Bool\n",
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
            "module App.MakeHash(K : App.HashContracts.Policy) : PublicResult exposing (answer, hash_answer)\nanswer : Bool\nlet answer = eq \"A\" \"A\"\nhash_answer : Int\nlet hash_answer = hash \"A\"\n\ntype module PublicResult =\n  answer : Bool\n  hash_answer : Int\n",
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
    for &(case, factory, instances) in hom_expression_cases() {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hom_expression_{case}"));
        let source = dir.join("App");
        write_hom_expression_sources(&source, factory, instances);
        if matches!(case, "outer_type_capture" | "dependent_factory_member") {
            fs::write(source.join("Main.fl"), "module App.Main exposing (main)\nimport App.Instances.Chosen as Chosen\nanswer : Int\nlet answer = Chosen.value + 1\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show answer)\n").expect("consumer of the dependent value");
        }
        match case {
            "private_generic" => {
                fs::write(source.join("Main.fl"), "module App.Main exposing (main)\nimport App.Instances.Chosen as Chosen\nbox : Chosen.Box Int\nlet box = Chosen.box\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (Chosen.unbox box + 1))\n").expect("consumer of the inferred type constructor");
            }
            "private_nominals" => {
                fs::write(source.join("Main.fl"), "module App.Main exposing (main)\nimport App.Instances.Chosen as Chosen\nimport App.Instances.Checked as Checked\nmain : Unit ->{} Unit\nlet main () =\n  let layer = Layer.succeed Chosen.Clock { now = fun () -> 41 }\n  let value = provide layer { Chosen.read () }\n  Platform.stdout_line (show (value + Checked.answer))\n").expect("consumer of inferred nominal members");
            }
            _ => {}
        }
        assert_hom_execution(&source, &dir, case, b"42\n");
    }
}

fn hom_expression_cases() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "private_generic",
            "private module App.Factory(K : App.Contracts.Input) exposing (Box, box, unbox)\ntype Box a = Box a\nbox : Box Int\nlet box = Box K.seed\nunbox : Box a -> a\nlet unbox (Box value) = value\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.Argument)\n",
        ),
        (
            "private_nominals",
            "private module App.Factory(K : App.Contracts.Input) exposing (Equality(..), equality, Clock, read)\nclass Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun a b -> a == b }\nservice Clock =\n  now : Unit ->{} Int\nread : Unit ->{Clock} Int\nlet read () = Clock.now ()\n",
            "module App.Instances exposing (Chosen, Checked)\nmodule Chosen = App.Factory(App.Argument)\nmodule Checked = App.UseEquality(Chosen)\n",
        ),
        (
            "private_returned",
            "private module App.Factory(K : App.Contracts.Input) = functor (J : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + J.seed\n",
            "module App.Instances exposing (Chosen)\nmodule Selected = App.Factory(App.Other)\nmodule Chosen = Selected(App.Argument)\n",
        ),
        (
            "captured_import",
            "module App.Factory exposing (Make)\nimport App.Helpers exposing (offset)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + offset\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory.Make(App.Argument)\n",
        ),
        (
            "header_alias",
            "module App.Factory exposing (Make)\nimport App.Contracts as Contracts\nmodule Make(K : Contracts.Input) : Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory.Make(App.Argument)\n",
        ),
        (
            "parameter_ascription",
            "module App.Factory(F : App.Contracts.RichFactory)(K : App.Contracts.Input) : App.Contracts.Output\nmodule Narrow : App.Contracts.Factory = F\nmodule Built = Narrow(K)\nanswer : Int\nlet answer = Built.answer\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.MakeRich)(App.Argument)\n",
        ),
        (
            "application_ascription",
            "module App.Factory(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen : App.Contracts.Output = App.Factory(App.Argument)\n",
        ),
        (
            "file_structure",
            "module App.Factory(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.Argument)\n",
        ),
        (
            "dependent_member",
            "module App.Factory(K : App.Contracts.Container) : App.Contracts.Output\nanswer : Int\nlet answer = K.M.bump K.value\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.Container)\n",
        ),
        (
            "dependent_factory_member",
            "module App.Factory(N : App.Contracts.SharedNamespace)(K : App.Contracts.Input) : App.Contracts.SharedOutput with type Value = N.M.Item = N.Build(K)\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.Shared)(App.Argument)\n",
        ),
        (
            "outer_type_capture",
            "module App.Factory(N : App.Contracts.TypeSpace)(K : App.Contracts.Input) : App.Contracts.SharedOutput with type Value = N.Item = N.Build(K)\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.TypedShared)(App.Argument)\n",
        ),
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
        (
            "shadowed_parameter",
            "module App.Factory(K : App.Contracts.Input) : App.Contracts.Factory = functor (K : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + 1\n",
            "module App.Instances exposing (Chosen)\nmodule Selected = App.Factory(App.Other)\nmodule Chosen = Selected(App.Argument)\n",
        ),
        (
            "grouped",
            "module App.Factory(A : App.Contracts.Input, B : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = A.seed + B.seed\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory(App.Argument, App.Other)\n",
        ),
    ]
}

fn write_hom_expression_sources(source: &Path, factory: &str, instances: &str) {
    fs::create_dir_all(source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Input, Output, Factory, Cell, Container, SharedOutput, SharedNamespace, TypeSpace, Rich, RichFactory, OwnEquality)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module RichFactory = (K : Input) -> Rich\ntype module Cell =\n  type Item\n  bump : Item -> Int\ntype module Container =\n  module M : Cell\n  value : M.Item\ntype module SharedOutput =\n  type Value\n  value : Value\n  answer : Int\ntype module SharedNamespace =\n  module M : Cell\n  module Build : (K : Input) -> SharedOutput with type Value = M.Item\ntype module TypeSpace =\n  type Item\n  module Build : (K : Input) -> SharedOutput with type Value = Item\ntype module OwnEquality =\n  class Equality a =\n    equal : a -> a -> Bool\n  equality : Equality.Dict String\n  instance Equality String from equality\n",
        ),
        (
            "Cell.fl",
            "module App.Cell exposing (Item, bump)\ntype alias Item = Int\nbump : Item -> Int\nlet bump value = value + 1\n",
        ),
        (
            "Container.fl",
            "module App.Container exposing (M, value)\nmodule M = App.Cell\nvalue : Int\nlet value = 41\n",
        ),
        (
            "Shared.fl",
            "module App.Shared exposing (M, Build)\nmodule M = App.Cell\nmodule Build = App.SharedBuild\n",
        ),
        (
            "TypedShared.fl",
            "module App.TypedShared exposing (Item, Build)\ntype alias Item = Int\nmodule Build = App.SharedBuild\n",
        ),
        (
            "SharedBuild.fl",
            "module App.SharedBuild(K : App.Contracts.Input) : App.Contracts.SharedOutput with type Value = App.Cell.Item\nimport App.Cell\ntype alias Value = App.Cell.Item\nvalue : Value\nlet value = K.seed\nanswer : Int\nlet answer = App.Cell.bump value\n",
        ),
        (
            "Argument.fl",
            "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
        ),
        (
            "Make.fl",
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
        ),
        (
            "Other.fl",
            "module App.Other exposing (seed)\nseed : Int\nlet seed = 1\n",
        ),
        (
            "Helpers.fl",
            "module App.Helpers exposing (offset)\noffset : Int\nlet offset = 1\n",
        ),
        (
            "MakeRich.fl",
            "module App.MakeRich(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n",
        ),
        (
            "UseEquality.fl",
            "module App.UseEquality(K : App.Contracts.OwnEquality) : App.Contracts.Output\nanswer : Int\nlet answer = if K.equal \"x\" \"x\" then 1 else 0\n",
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
}

fn assert_hom_execution(source: &Path, dir: &Path, case: &str, expected: &[u8]) {
    for profile in ["debug", "release"] {
        for emit in ["js", "ts", "go"] {
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
            let output = if emit == "go" {
                Command::new(out.join(executable(MAIN_ENTRY)))
                    .output()
                    .expect("Go executable")
            } else {
                Command::new("node")
                    .arg(out.join(runner_path(emit == "ts")))
                    .output()
                    .expect("Node")
            };
            assert!(
                output.status.success(),
                "{case}/{emit}/{profile}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected, "{case}/{emit}/{profile}");
        }
    }
}

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
