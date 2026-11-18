#[test]
#[ignore = "needs Node and Go; run `just e2e`"]
fn abstract_class_requirement_method_runs_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md2_abstract_class_method");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Policy)\ntype module Policy =\n  class Equality a =\n    equal : a -> a -> Bool\n  equality : Equality.Dict String\n  instance Equality String from equality\n",
        ),
        (
            "Argument.fl",
            "module App.Argument exposing (Equality(..), equality)\nclass Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun _ _ -> false }\n",
        ),
        (
            "Make.fl",
            "module App.Make(K : App.Contracts.Policy) : PublicResult exposing (answer)\nanswer : Bool\nlet answer = K.equal \"A\" \"A\"\n\ntype module PublicResult =\n  answer : Bool\n",
        ),
        (
            "Parent.fl",
            "module App.Parent exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.Parent.Chosen exposing (answer)\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (if answer == false then \"ok\" else \"wrong\")\n",
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
            assert_eq!(output.stdout, b"ok\n", "{emit}/{profile}");
        }
    }
}

#[test]
#[ignore = "needs Node and Go; run `just e2e`"]
fn unpack_uses_its_packed_dictionary_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md2_unpack_dictionary");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Policy)\ntype module Policy =\n  type Key = String\n  equality : Eq.Dict Key\n  instance Eq Key from equality\n",
        ),
        (
            "Argument.fl",
            "module App.Argument exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> false }\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.Contracts exposing (Policy)\nimport App.Argument as Argument\npacked : Module Policy\nlet packed = pack Argument as Policy\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (unpack packed as K in eq \"A\" \"A\"))\n",
        ),
    ] {
        fs::write(source.join(name), text).expect("write source");
    }
    let main = source.join("Main.fl");
    for emit in ["js", "go"] {
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
        assert_eq!(output.stdout, b"false\n", "{emit}");
    }
}

#[test]
#[ignore = "needs Node and Go; run `just e2e`"]
fn existential_module_result_opens_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md2_existential_result");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in [
        (
            "Contracts.fl",
            "module App.Contracts exposing (Box)\ntype module Box =\n  type Key\n  value : Key\n",
        ),
        (
            "Argument.fl",
            "module App.Argument exposing (Key, value)\ntype alias Key = Int\nvalue : Key\nlet value = 42\n",
        ),
        (
            "Packed.fl",
            "module App.Packed exposing (result)\nimport App.Contracts exposing (Box)\nimport App.Argument as Arg\nresult : exists K : Box. K.Key\nlet result = unpack (pack Arg as Box) as K in K.value\n",
        ),
        (
            "Use.fl",
            "module App.Use exposing (answer)\nimport App.Packed exposing (result)\nanswer : Int\nlet answer = unpack result as K, item in (fun _ -> 42) item\n",
        ),
        (
            "Main.fl",
            "module App.Main exposing (main)\nimport App.Use exposing (answer)\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show answer)\n",
        ),
    ] {
        fs::write(source.join(name), text).expect("write source");
    }
    let main = source.join("Main.fl");
    for emit in ["js", "go"] {
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
        assert_eq!(output.stdout, b"42\n", "{emit}");
    }
}

/// An importer reaches instances an inline child declares through a value
/// of the parent: a derived `Show`, a derived instance that takes its
/// class's default and one written without methods. Its build must name
/// them where the parent's file defines them, under the child's prefix,
/// not in a module of the child's path that no build emits.
#[test]
#[ignore = "needs Node and Go; run `just e2e`"]
fn an_importer_calls_the_instances_of_an_inline_child_on_js_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inline_child_instances");
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    let parent = "module App.Parent exposing (Describe(..), pair, box)

class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = \"<\" ++ show x ++ \">\"

module Child exposing (Pair(..), Box(..)) =
  type Pair = Pair Int Int
    deriving Show, Describe
  type Box = Box Int
    deriving Show
  instance Describe Box

pair : Child.Pair
let pair = Child.Pair 1 2

box : Child.Box
let box = Child.Box 3
";
    let main = "module App.Main exposing (main)

import App.Parent exposing (Describe(..), pair, box)

main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line (describe pair ++ \" \" ++ show pair ++ \" \" ++ describe box)
";
    fs::write(source.join("Parent.fl"), parent).expect("write source");
    fs::write(source.join("Main.fl"), main).expect("write source");
    for emit in ["js", "go"] {
        let out = dir.join(format!("out-{emit}"));
        let build = Command::new(env!("CARGO_BIN_EXE_flick"))
            .arg("build")
            .arg(source.join("Main.fl"))
            .args(["--emit", emit, "--out-dir"])
            .arg(&out)
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
        // `show` writes a constructor and its fields apart by spaces, and
        // the default of `describe` puts what `show` writes in brackets
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "<Pair 1 2> Pair 1 2 <Box 3>\n",
            "{emit}"
        );
    }
}
