//! Unsupported hosts fail at their invocation, not at callable lookup.
use flick_model::{eval::Outcome, run};

#[test]
fn unknown_foreign_uses_host_arity_instead_of_source_arrow_count() {
    for arity in [1, 2] {
        let source = format!(
            r#"module UnknownHost exposing (main)
external : String -> String -> String
foreign pure external from "./unknown.mjs" "external"
  arity {arity}
main : Unit ->{{Host.Console}} Unit
let main () =
  let f = external
  Platform.stdout_line "looked up"
  let partial = f "first"
  Platform.stdout_line "after first"
  let _ = partial "second"
  ()
"#
        );
        let modules = run::compile_source(&source).expect("accepted foreign convention");
        let ran = run::run_main(
            &run::program(&modules),
            &run::main_of(&modules).expect("main"),
            10000,
        );
        let expected = if arity == 1 {
            "looked up\n"
        } else {
            "looked up\nafter first\n"
        };
        assert_eq!(
            ran.stdout, expected,
            "host arity {arity}: {:?}",
            ran.outcome
        );
        assert!(
            matches!(ran.outcome, Outcome::Stuck(message) if message == "no native for `external` (./unknown.mjs external)")
        );
    }
}

#[test]
fn an_unsupported_call_inside_a_task_stays_cold_until_awaited() {
    for awaited in [false, true] {
        let force = if awaited { "await cold" } else { "()" };
        let source = format!(
            r#"module ColdForeign exposing (main)
external : Unit ->{{}} Unit
foreign external from "./unknown.mjs" "external"
  arity 0
main : Unit ->{{Host.Console}} Unit
let main () =
  let cold = task {{ external () }}
  Platform.stdout_line "constructed"
  {force}
  Platform.stdout_line "after"
"#
        );
        let modules = run::compile_source(&source).expect("accepted cold external call");
        let ran = run::run_main(
            &run::program(&modules),
            &run::main_of(&modules).expect("main"),
            10000,
        );
        if awaited {
            assert_eq!(ran.stdout, "constructed\n");
            assert!(matches!(ran.outcome, Outcome::Stuck(_)));
        } else {
            assert_eq!(ran.stdout, "constructed\nafter\n");
            assert!(matches!(ran.outcome, Outcome::Value(_)));
        }
    }
}
