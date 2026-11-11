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
