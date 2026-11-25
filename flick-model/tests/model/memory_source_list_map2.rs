//! Genuine pure Applicative List source on the counted path.
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
answer : Unit -> List String
let answer () = map2 (fun left right -> left ++ ":" ++ right) ["L1", "L2"] ["R1", "R2"]
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
    .expect("counted List map2 lowering")
}
#[test]
fn genuine_list_map2_combines_left_then_right() {
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
        for (index, expected) in ["L1:R1", "L1:R2", "L2:R1", "L2:R2"].into_iter().enumerate() {
            assert_eq!(
                heap.string(heap.field(&ran.value, index).expect("ordered output")),
                Ok(expected)
            );
        }
        heap.drop_value(ran.value).expect("result release");
        let stats = heap.finish().expect("actual zero occupied heap");
        assert_eq!(stats.allocations, stats.frees);
    }
}

use crate::causes;
const CASES: &str = r#"module App.Map2 exposing (duplicates, cold, eager, combine, identity, order, eager_full, eager_f)
spin : Unit -> a
let rec spin () = spin ()
wrong_first : String -> String -> String
let wrong_first _ = spin ()
wrong_second : String -> String -> String
let wrong_second _ _ = spin ()
duplicates : Unit -> List String
let duplicates () = map2 (fun left right -> left ++ ":" ++ right) ["L", "L"] ["A", "B", "A"]
cold : String -> List String
let cold captured =
  let partial = map2 (fun left right -> captured ++ wrong_second left right)
  let _partial_left = partial ["left"]
  map2 wrong_first [] ["right"] ++ map2 wrong_first ["left"] [] ++ map2 wrong_second [] []
argument : String -> List String ->{} List String
let argument label value =
  Host.raw_stdout_line label
  value
factory : Unit -> (String -> String -> String)
let factory () =
  wrong_first
factory_good : Unit -> (String -> String -> String)
let factory_good () =
  fun a b -> a ++ ":" ++ b
eager_full : Unit ->{} List String
let eager_full () = map2 (factory_good ()) (argument "left" ["L1", "L2"]) (argument "right" ["R1", "R2"])
factory_error : Unit -> (String -> String -> String)
let factory_error () = if Int.unsafe_div 1 0 == 0 then wrong_first else wrong_second
eager_f : Unit ->{} List String
let eager_f () = map2 (factory_error ()) (argument "left" []) (argument "right" ["right"])
eager : Unit ->{} List String
let eager () = map2 (factory ()) (argument "left" []) (argument "right" ["right"])
combine : String -> List String -> List String -> List String
let combine captured left right = map2 (fun a b -> captured ++ ":" ++ a ++ "/" ++ b) left right
identity : Unit ->{} Bool
let identity () =
  let left = Ref.make "left"
  let right = Ref.make "right"
  let result = map2 (fun first _ -> first) [left] [right, right]
  match result with
  | [first, second] ->
    Ref.set first "changed"
    Ref.same first left && Ref.same second left && Ref.get left == "changed" && Ref.get right == "right"
  | _ -> false
order : Int -> List Int
let order target = map2 (fun left right ->
  let index = left * 2 + right
  if index < target then index else if index == target then Int.unsafe_div 1 0 else Int.unsafe_rem 1 0) [0, 1] [0, 1]
"#;
fn assert_strings(heap: &Heap, value: &Value, expected: &[&str]) {
    assert_eq!(heap.tag(value), Ok("$list"));
    for (index, text) in expected.iter().enumerate() {
        assert_eq!(
            heap.string(heap.field(value, index).expect("ordered field")),
            Ok(*text)
        );
    }
    assert!(
        heap.field(value, expected.len()).is_err(),
        "exact multiplicity, no evidence suffix"
    );
}
#[test]
fn cartesian_multiplicity_cold_partial_calls_eager_arguments_and_reference_aliases() {
    for release in [false, true] {
        for (name, expected) in [
            ("duplicates", vec!["L:A", "L:B", "L:A", "L:A", "L:B", "L:A"]),
            ("cold", vec![]),
            ("eager", vec![]),
            ("eager_full", vec!["L1:R1", "L1:R2", "L2:R1", "L2:R2"]),
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
            .expect("empty input never invokes either callback stage");
            assert_strings(&heap, &ran.value, &expected);
            assert_eq!(
                ran.stdout,
                if name.starts_with("eager") {
                    "left\nright\n"
                } else {
                    ""
                }
            );
            heap.drop_value(ran.value).expect("return release");
            heap.finish().expect("zero occupied heap");
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
        .expect("reference identity");
        assert_eq!(ran.value, Value::Bool(true));
        heap.drop_value(ran.value).expect("release");
        heap.finish().expect("reference payloads released");
    }
}
#[test]
fn every_cartesian_prefix_preserves_capture_and_both_payload_sides() {
    use flick_model::memory::{Error, ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "combine", release);
        for (left_len, right_len) in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            let mut completed = false;
            for fuel in 0..=8192 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let capture = heap.dup(&external).expect("capture alias");
                let mut list = |len| {
                    let fields = (0..len)
                        .map(|_| heap.dup(&external).expect("payload alias"))
                        .collect();
                    heap.construct("$list", fields).expect("list input")
                };
                let left = list(left_len);
                let right = list(right_len);
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
                        assert_strings(
                            &heap,
                            &value,
                            &vec!["outside:outside/outside"; left_len * right_len],
                        );
                        heap.drop_value(value).expect("result release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => {
                        let detail = format!("{other:?}");
                        other.release(&mut heap).expect("outcome release");
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
                        "listMap2 release={release} lengths=({left_len},{right_len}) terminal={fuel}"
                    );
                    break;
                }
            }
            assert!(completed);
        }
    }
}
#[test]
fn every_callback_position_observes_left_outer_right_inner_order() {
    use flick_model::memory::{Error, ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "order", release);
        for target in 0..4 {
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Map2", "order"),
                vec![Value::Int(target)],
                100_000,
            );
            assert_eq!(report.stdout, "");
            let ProgramOutcome::Failed(cause) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "exact ordered callback defect")
            };
            assert_eq!(
                causes::defect_error(&mut heap, cause),
                Error::IntDivisionByZero { remainder: false },
                "target={target} release={release}: a later pair or reversed row would produce remainder defect"
            );
            heap.finish()
                .expect("prior mapped values, captures and tails released");
        }
    }
}

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
