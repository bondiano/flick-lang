#[cfg(feature = "ide")]
#[test]
fn hom_private_factories_infer_returned_functor_signatures() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Return",
                "private module App.Return(K : App.Contracts.Input) = functor (J : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + J.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Selected = App.Return(App.Argument)\nmodule Chosen = Selected(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_private_inference_retains_kinds_and_generative_nominal_members() {
    use crate::diagnostics::codes;
    for (member, declarations, use_source, incompatible) in [
        (
            "Box",
            "type Box a = Box a\nbox : Box Int\nlet box = Box K.seed",
            "box : Chosen.Box Int\nlet box = Chosen.box",
            false,
        ),
        (
            "Box",
            "type Box a = Box a\nbox : Box Int\nlet box = Box K.seed",
            "box : Other.Box Int\nlet box = Chosen.box",
            true,
        ),
        (
            "Equality(..), Clock",
            "class Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun a b -> a == b }\nservice Clock =\n  now : Unit ->{} Int\nread : Unit ->{Clock} Int\nlet read () = Clock.now ()",
            "equality : Chosen.Equality.Dict String\nlet equality = Chosen.equality\nread : Unit ->{Chosen.Clock} Int\nlet read = Chosen.read",
            false,
        ),
        (
            "Equality(..), Clock",
            "class Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun a b -> a == b }\nservice Clock =\n  now : Unit ->{} Int\nread : Unit ->{Clock} Int\nlet read () = Clock.now ()",
            "equality : Other.Equality.Dict String\nlet equality = Chosen.equality",
            true,
        ),
    ] {
        let exports = if member == "Box" {
            "Box, box, answer"
        } else {
            "Equality(..), Clock, equality, read, answer"
        };
        let factory = format!(
            "private module App.Factory(K : App.Contracts.Input) exposing ({exports})\n{declarations}\nanswer : Int\nlet answer = K.seed + 1\n"
        );
        let root = format!(
            "module App.Root exposing (..)\nmodule Chosen = App.Factory(App.Argument)\nmodule Other = App.Factory(App.Argument)\n{use_source}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                ("Factory", &factory),
                ("Root", &root),
            ],
            "Root",
        );
        if incompatible {
            assert!(
                analysis.diagnostics.iter().any(|d| d.code == codes::T003),
                "{member}: {:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{member}: {:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_private_inference_preserves_parameterized_manifest_equalities() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Factory",
                "private module App.Factory(K : App.Contracts.Input) exposing (Box, box)\ntype alias Box a = List a\nbox : Box Int\nlet box = [K.seed]\n",
            ),
            (
                "Root",
                "module App.Root exposing (box)\nmodule First = App.Factory(App.Argument)\nmodule Second = App.Factory(App.Argument)\nbox : Second.Box Int\nlet box = First.box\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_parameterized_manifest_members_compare_bodies_under_their_binders() {
    use crate::diagnostics::codes;
    for (body, value, rejected) in [("List x", "[]", false), ("Option x", "None", true)] {
        let make = format!(
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\ntype alias Items x = {body}\nempty : forall x. Items x\nlet empty = {value}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  type Items a = List a\n  empty : forall a. Items a\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                ("Make", &make),
                (
                    "Root",
                    "module App.Root exposing (empty)\nmodule Chosen = App.Make(App.Argument)\nempty : List Int\nlet empty = Chosen.empty\n",
                ),
            ],
            "Root",
        );
        if rejected {
            assert!(
                analysis.diagnostics.iter().any(|d| d.code == codes::T061),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_functors_capture_earlier_local_signatures_types_and_values() {
    use crate::diagnostics::codes;
    for late in [false, true] {
        let offset = "offset : Int\nlet offset = 1\n";
        let root = format!(
            "module App.Root exposing (Chosen, Token)\nopaque type alias Token = Int\nwrap : Int -> Token\nlet wrap value = value\n{}type module Input =\n  seed : Int\ntype module Output =\n  type Value = Token\n  value : Value\nmodule Make(K : Input) : Output =\n  type alias Value = Token\n  value : Value\n  let value = wrap (K.seed + offset)\n{}module Chosen = Make(App.Argument)\n",
            if late { "" } else { offset },
            if late { offset } else { "" }
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        if late {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.code == codes::T001 && d.message.contains("offset")),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_functors_capture_values_of_an_enclosing_generic_functor() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Namespace =\n  module Make : Input -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Outer",
                "module App.Outer(K : App.Contracts.Input) : App.Contracts.Namespace\noffset : Int\nlet offset = K.seed\nmodule Make(J : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = offset + J.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Namespace = App.Outer(App.Argument)\nmodule Chosen = Namespace.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_factories_capture_a_parameter_dictionary_in_preceding_values() {
    let (sources, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                r"module App.Contracts exposing (Policy, Output, Factory, Namespace)
type module Policy =
  type Key = String
  equality : Eq.Dict Key
  instance Eq Key from equality
type module Output =
  answer : Bool
type module Factory = (K : Policy) -> Output
type module Namespace =
  module Make : Factory
",
            ),
            (
                "Always",
                r"module App.Always exposing (Key, equality)
type alias Key = String
equality : Eq.Dict Key
let equality = Eq.dictionary { eq = fun _ _ -> true }
",
            ),
            (
                "Never",
                r"module App.Never exposing (Key, equality)
type alias Key = String
equality : Eq.Dict Key
let equality = Eq.dictionary { eq = fun _ _ -> false }
",
            ),
            (
                "Make",
                r#"module App.Make(A : App.Contracts.Policy)(B : App.Contracts.Policy) : App.Contracts.Output
answer : Bool
let answer = eq "x" "x"
"#,
            ),
            (
                "Outer",
                r#"module App.Outer(A : App.Contracts.Policy) : App.Contracts.Factory = functor (B : App.Contracts.Policy) ->
  answer : Bool
  let answer = eq "x" "x"
"#,
            ),
            (
                "Capture",
                r#"module App.Capture(A : App.Contracts.Policy) : App.Contracts.Namespace
prior : Bool
let prior = eq "x" "x"
module Make(B : App.Contracts.Policy) : App.Contracts.Output =
  answer : Bool
  let answer = if prior then eq "x" "x" else true
"#,
            ),
            (
                "Instances",
                r"module App.Instances exposing (Curried, Nested, Captured, CapturedOther)
module Curried = App.Make(App.Always)(App.Never)
module Nested = App.Outer(App.Always)(App.Never)
module First = App.Capture(App.Always)
module Second = App.Capture(App.Never)
module Captured = First.Make(App.Never)
module CapturedOther = Second.Make(App.Always)
",
            ),
            (
                "Main",
                r#"module App.Main exposing (main)
import App.Instances.Curried as Curried
import App.Instances.Nested as Nested
import App.Instances.Captured as Captured
import App.Instances.CapturedOther as CapturedOther
main : Unit ->{} Unit
let main () = Platform.stdout_line (show Curried.answer ++ " " ++ show Nested.answer ++ " " ++ show Captured.answer ++ " " ++ show CapturedOther.answer ++ " " ++ show (eq "x" "x"))
"#,
            ),
        ],
        "Main",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}\n{}",
        analysis.diagnostics,
        sources
            .files()
            .filter(|s| s.path.contains("First") || s.path.contains("Second"))
            .map(|s| format!("{}\n{}", s.path, s.text))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_private_inference_preserves_exported_adt_constructors() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Factory",
                "private module App.Factory(K : App.Contracts.Input) exposing (Box(..), box)\ntype Box a = Box a\nbox : Box Int\nlet box = Box K.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Chosen = App.Factory(App.Argument)\nanswer : Int\nlet answer = match Chosen.Box 42 with\n  | Chosen.Box n -> n\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_aliases_resolve_to_canonical_module_paths_once() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Other",
                "module App.Other exposing (seed)\nseed : Int\nlet seed = 0\n",
            ),
            (
                "Factory",
                "module App.Factory(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nimport App.Factory as Factory\nimport App.Argument as Actual\nimport App.Other as App\nmodule Chosen = Factory(Actual)\nanswer : Int\nlet answer = Chosen.answer + App.seed\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}
