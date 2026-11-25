//! Genuine pure Applicative Result source on the counted path.
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::{
    memory::{Heap, Value, execute_program},
    run,
};
use std::panic::catch_unwind;
const SOURCE: &str = r#"module App.Map2 exposing (answer)
answer : Unit -> String
let answer () = match map2 (fun left right -> left ++ ":" ++ right) (Ok "L" : Result String String) (Ok "R") with | Ok text -> text | Err text -> "wrong " ++ text
"#;
fn program(source: &str, name: &str, release: bool) -> Program {
    let modules = run::compile_source(source).expect("genuine checked source");
    let ir: Vec<_> = modules
        .iter()
        .map(|m| {
            if release {
                build::optimized(&modules, m, false)
            } else {
                (*m.ir).clone()
            }
        })
        .collect();
    perceus::lower(
        ir.iter().chain(build::builtin_modules()),
        &Name::new("App.Map2", name),
    )
    .expect("counted Result map2 lowering")
}
#[test]
fn genuine_result_map2_combines_left_then_right() {
    let programs = [false, true].map(|release| catch_unwind(|| program(SOURCE, "answer", release)));
    assert!(
        programs.iter().all(Result::is_ok),
        "both profiles support genuine map2"
    );
    for result in programs {
        let mut heap = Heap::new();
        let program = result.expect("both profiles lower");
        let ran = execute_program(
            &mut heap,
            &program,
            &Name::new("App.Map2", "answer"),
            vec![Value::Unit],
            100_000,
        )
        .expect("execution");
        assert_eq!(heap.string(&ran.value), Ok("L:R"));
        heap.drop_value(ran.value).expect("result release");
        let stats = heap.finish().expect("actual zero occupied heap");
        assert_eq!(stats.allocations, stats.frees);
    }
}

const CASES: &str = r#"module App.Map2 exposing (matrix, cold, eager, combine, identity, eager_full, eager_f)
render : Result String String -> String
let render value = match value with | Ok text -> "ok:" ++ text | Err text -> "err:" ++ text
pair : Result String String -> Result String String -> String
let pair left right = render (map2 (fun a b -> a ++ "/" ++ b) left right)
matrix : Unit -> String
let matrix () = pair (Ok "L") (Ok "R") ++ "|" ++ pair (Err "left") (Ok "R") ++ "|" ++ pair (Ok "L") (Err "right") ++ "|" ++ pair (Err "first") (Err "second")
spin : Unit -> a
let rec spin () = spin ()
wrong_first : String -> String -> String
let wrong_first _ = spin ()
wrong_second : String -> String -> String
let wrong_second _ _ = spin ()
cold : String -> String
let cold captured =
  let partial = map2 (fun left right -> captured ++ wrong_second left right)
  let _partial_left = partial (Ok "left" : Result String String)
  render (map2 wrong_first (Err "left") (Ok "R")) ++ "|" ++ render (map2 wrong_first (Ok "L") (Err "right")) ++ "|" ++ render (map2 wrong_second (Err "first") (Err "second")) ++ "|" ++ captured
argument : String -> Result String String ->{} Result String String
let argument label value =
  Host.raw_stdout_line label
  value
factory : Unit -> (String -> String -> String)
let factory () =
  wrong_first
factory_good : Unit -> (String -> String -> String)
let factory_good () =
  fun a b -> a ++ "/" ++ b
eager_full : Unit ->{} String
let eager_full () = render (map2 (factory_good ()) (argument "left" (Ok "L")) (argument "right" (Ok "R")))
factory_error : Unit -> (String -> String -> String)
let factory_error () = if Int.unsafe_div 1 0 == 0 then wrong_first else wrong_second
eager_f : Unit ->{} String
let eager_f () = render (map2 (factory_error ()) (argument "left" (Err "first")) (argument "right" (Err "second")))
eager : Unit ->{} String
let eager () = render (map2 (factory ()) (argument "left" (Err "first")) (argument "right" (Err "second")))
combine : String -> Result String String -> Result String String -> Result String String
let combine captured left right = map2 (fun a b -> captured ++ ":" ++ a ++ "/" ++ b) left right
identity : Unit ->{} Bool
let identity () =
  let left = Ref.make "left"
  let right = Ref.make "right"
  let result = map2 (fun _ _ -> right) (Err left : Result (Ref String) String) (Err right)
  match result with
  | Err chosen ->
    Ref.set chosen "changed"
    Ref.same chosen left && Ref.get left == "changed" && Ref.get right == "right"
  | Ok _ -> false
"#;
#[test]
fn result_left_bias_cold_stages_eager_arguments_and_error_payload_identity() {
    for release in [false, true] {
        for (name, expected) in [
            ("matrix", "ok:L/R|err:left|err:right|err:first"),
            ("cold", "err:left|err:right|err:first|kept"),
            ("eager", "err:first"),
            ("eager_full", "ok:L/R"),
        ] {
            let p = program(CASES, name, release);
            let mut heap = Heap::new();
            let input = if name == "cold" {
                heap.literal(&Lit::String("kept".into())).expect("capture")
            } else {
                Value::Unit
            };
            let ran = execute_program(
                &mut heap,
                &p,
                &Name::new("App.Map2", name),
                vec![input],
                100_000,
            )
            .expect("Err skips both callback stages, not arguments");
            assert_eq!(heap.string(&ran.value), Ok(expected));
            assert_eq!(
                ran.stdout,
                if name.starts_with("eager") {
                    "left\nright\n"
                } else {
                    ""
                }
            );
            heap.drop_value(ran.value).expect("return release");
            let stats = heap.finish().expect("zero occupied heap");
            assert_eq!(stats.allocations, stats.frees);
        }
        let p = program(CASES, "identity", release);
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &p,
            &Name::new("App.Map2", "identity"),
            vec![Value::Unit],
            100_000,
        )
        .expect("reference payloads");
        assert_eq!(ran.value, Value::Bool(true));
        heap.drop_value(ran.value).expect("result release");
        heap.finish().expect("all reference payloads released");
    }
}
#[test]
fn every_result_prefix_preserves_external_capture_and_payload_aliases() {
    use flick_model::memory::{Error, ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "combine", release);
        for (left_ok, right_ok) in [(true, true), (true, false), (false, true), (false, false)] {
            let mut completed = false;
            for fuel in 0..=4096 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let capture = heap.dup(&external).expect("capture alias");
                let mut result = |ok| {
                    let field = heap.dup(&external).expect("payload alias");
                    heap.construct(
                        if ok {
                            "Prelude.Result.Ok"
                        } else {
                            "Prelude.Result.Err"
                        },
                        vec![field],
                    )
                    .expect("ordinary input")
                };
                let left = result(left_ok);
                let right = result(right_ok);
                let report = execute_program_outcome(
                    &mut heap,
                    &p,
                    &Name::new("App.Map2", "combine"),
                    vec![capture, left, right],
                    fuel,
                );
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        assert_eq!(
                            heap.tag(&value),
                            Ok(if left_ok && right_ok {
                                "Prelude.Result.Ok"
                            } else {
                                "Prelude.Result.Err"
                            })
                        );
                        let field = heap.field(&value, 0).expect("one ordinary payload");
                        if left_ok && right_ok {
                            assert_eq!(heap.string(field), Ok("outside:outside/outside"));
                        } else {
                            assert_eq!(
                                field.object_id(),
                                external.object_id(),
                                "error PAYLOAD identity, not common wrapper identity"
                            );
                        }
                        assert!(heap.field(&value, 1).is_err(), "no evidence suffix");
                        heap.drop_value(value).expect("result release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => {
                        let detail = format!("{other:?}");
                        other
                            .release(&mut heap)
                            .expect("unexpected outcome release");
                        panic!("{detail}")
                    }
                }
                assert_eq!(heap.string(&external), Ok("outside"));
                assert_eq!(heap.reference_count(&external), Ok(1));
                assert_eq!(heap.stats().live_objects, 1);
                heap.drop_value(external).expect("external release");
                let stats = heap.finish().expect("actual zero occupied slots");
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!(
                        "resultMap2 release={release} ok=({left_ok},{right_ok}) terminal={fuel}"
                    );
                    break;
                }
            }
            assert!(completed);
        }
    }
}

use crate::causes;

#[test]
fn callback_expression_is_eager_before_either_argument_even_on_the_empty_path() {
    use flick_model::memory::{Error, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "eager_f", release);
        let mut heap = Heap::new();
        let report = execute_program_outcome(
            &mut heap,
            &p,
            &Name::new("App.Map2", "eager_f"),
            vec![Value::Unit],
            100_000,
        );
        assert_eq!(report.stdout, "");
        causes::assert_defect(
            &mut heap,
            report.outcome,
            &Error::IntDivisionByZero { remainder: false },
        );
        heap.finish()
            .expect("factory defect releases every internal root");
    }
}
