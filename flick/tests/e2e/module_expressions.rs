#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn anonymous_nested_and_returned_parameterized_modules_execute_on_js_and_go() {
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
            "private module App.Factory(K : App.Contracts.Input) = module (J : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + J.seed\n",
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
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory((module (K : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + 1\n))(App.Argument)\n",
        ),
        (
            "nested",
            "module App.Factory exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
            "module App.Instances exposing (Chosen)\nmodule Chosen = App.Factory.Make(App.Argument)\n",
        ),
        (
            "returned_anonymous",
            "module App.Factory(F : App.Contracts.Factory) : App.Contracts.Factory = module (K : App.Contracts.Input) ->\n  module Built = F(K)\n  answer : Int\n  let answer = Built.answer\n",
            "module App.Instances exposing (Chosen)\nmodule Selected = App.Factory(App.Make)\nmodule Chosen = App.Instances.Selected(App.Argument)\n",
        ),
        (
            "shadowed_parameter",
            "module App.Factory(K : App.Contracts.Input) : App.Contracts.Factory = module (K : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + 1\n",
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
            "module App.Outer(A : App.Contracts.Policy) : App.Contracts.Factory = module (B : App.Contracts.Policy) ->\n  answer : Bool\n  let answer = eq \"x\" \"x\"\n",
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
fn nested_static_parameter_contract_dictionaries_execute_on_js_ts_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("nested_static_dictionary");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Policy, Container, Output)\ntype module Policy =\n  type Item\n  item : Item\n  equality : Eq.Dict Item\n  instance Eq Item from equality\ntype module Container =\n  module Policy : Policy\ntype module Output =\n  answer : Bool\n",
        ),
        (
            "Policy.fl",
            "module App.Policy exposing (Item, item, equality)\ntype Item = Item\nitem : Item\nlet item = Item\nequality : Eq.Dict Item\nlet equality = Eq.dictionary { eq = fun _ _ -> true }\n",
        ),
        (
            "Container.fl",
            "module App.Container exposing (Policy)\nmodule Policy = App.Policy\n",
        ),
        (
            "Make.fl",
            "module App.Make(N : App.Contracts.Container) : App.Contracts.Output\nanswer : Bool\nlet answer = eq N.Policy.item N.Policy.item\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nmodule Chosen = App.Make(App.Container)\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (if Chosen.answer then \"42\" else \"0\")\n",
        ),
    ] {
        fs::write(source.join(name), text).expect("source");
    }
    assert_hom_execution(&source, &dir, "nested-static-dictionary", b"42\n");
}
