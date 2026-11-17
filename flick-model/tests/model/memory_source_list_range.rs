//! Checked Int half-open ranges through closed ordinary counted code.
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
const SOURCE: &str = r"module App.Range exposing (answer)
answer : Unit -> List Int
let answer () = List.range (-2) 2
";
fn program(source: &str, name: &str, release: bool) -> Program {
    let modules = run::compile_source(source).expect("genuine source checks");
    let ir: Vec<_> = modules
        .iter()
        .map(|m| {
            if release {
                build::optimized(&modules, m, false)
            } else {
                m.ir.clone()
            }
        })
        .collect();
    perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        &Name::new("App.Range", name),
    )
    .expect("counted checked Int range")
}
#[test]
fn genuine_range_is_ascending_and_half_open_in_both_profiles() {
    let programs = [false, true].map(|release| catch_unwind(|| program(SOURCE, "answer", release)));
    assert!(
        programs.iter().all(Result::is_ok),
        "both profiles lower genuine range"
    );
    for p in programs {
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &p.expect("both lower"),
            &Name::new("App.Range", "answer"),
            vec![Value::Unit],
            100_000,
        )
        .expect("range executes");
        assert_eq!(heap.tag(&ran.value), Ok("$list"));
        for (index, expected) in [-2, -1, 0, 1].into_iter().enumerate() {
            assert_eq!(heap.field(&ran.value, index), Ok(&Value::Int(expected)));
        }
        assert!(heap.field(&ran.value, 4).is_err());
        heap.drop_value(ran.value).expect("result release");
        heap.finish().expect("actual zero occupied slots");
    }
}

use crate::causes;
const CASES: &str = r#"module App.Range exposing (bounds, eager, first_fault, second_fault, captured, identity, holding, cold)
import Arena
bounds : Int -> Int -> List Int
let bounds start stop = List.range start stop
argument : String -> Int ->{} Int
let argument label value =
  Platform.stdout_line label
  value
eager : Unit ->{} List Int
let eager () = List.range (argument "left" 4) (argument "right" 2)
first_fault : Unit ->{} List Int
let first_fault () = List.range (Int.unsafe_div 1 0) (argument "unreachable" 3)
second_fault : Unit ->{} List Int
let second_fault () = List.range (argument "left" 2) (Int.unsafe_rem 1 0)
captured : String -> Unit -> List Int
let captured text () =
  let start = String.length text
  let partial = List.range start
  partial (start + 2)
cold : Unit -> (Int -> List Int)
let cold () = List.range (-2147483647 - 1)
identity : Unit ->{} Bool
let identity () =
  let left = Ref.make 2
  let right = Ref.make 4
  let partial = List.range (Ref.get left)
  let values = partial (Ref.get right)
  Ref.set left 8
  values == [2, 3] && Ref.get left == 8 && Ref.get right == 4
holding : Unit ->{} List Int
let holding () =
  let retained = scope s {
    let view = Arena.alloc (Arena.make ()) "retired"
    [fun () -> ignore view, fun () -> ignore view]
  }
  List.range 0 (List.length retained)
"#;
fn assert_ints(heap: &Heap, value: &Value, expected: &[i32]) {
    assert_eq!(heap.tag(value), Ok("$list"));
    for (index, n) in expected.iter().enumerate() {
        assert_eq!(heap.field(value, index), Ok(&Value::Int(*n)));
    }
    assert!(
        heap.field(value, expected.len()).is_err(),
        "exact ordinary element count, no evidence suffix"
    );
}
#[test]
fn all_endpoint_prefixes_finish_without_overflow_or_collecting_external_roots() {
    use flick_model::memory::{Error, ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "bounds", release);
        for (from, until) in [
            (-3, 3),
            (-1, 0),
            (0, 1),
            (0, 0),
            (1, -1),
            (i32::MIN, i32::MIN + 2),
            (i32::MAX - 2, i32::MAX),
            (i32::MAX, i32::MAX),
            (i32::MAX, i32::MIN),
        ] {
            let expected: Vec<_> = (from..until).collect();
            let mut completed = false;
            for fuel in 0..=4096 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let report = execute_program_outcome(
                    &mut heap,
                    &p,
                    &Name::new("App.Range", "bounds"),
                    vec![Value::Int(from), Value::Int(until)],
                    fuel,
                );
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        assert_ints(&heap, &value, &expected);
                        heap.drop_value(value).expect("result release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => {
                        let detail = format!("{other:?}");
                        other.release(&mut heap).expect("unexpected release");
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
                    eprintln!("range release={release} [{from},{until}) terminal={fuel}");
                    break;
                }
            }
            assert!(completed);
        }
    }
}
#[test]
fn arguments_are_eager_left_to_right_even_when_empty_and_defects_keep_their_order() {
    use flick_model::memory::{Error, ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        for (name, expected_stdout, error) in [
            ("eager", "left\nright\n", None),
            ("first_fault", "", Some(false)),
            ("second_fault", "left\n", Some(true)),
        ] {
            let p = program(CASES, name, release);
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Range", name),
                vec![Value::Unit],
                100_000,
            );
            assert_eq!(report.stdout, expected_stdout);
            if let Some(remainder) = error {
                causes::assert_defect(
                    &mut heap,
                    report.outcome,
                    &Error::IntDivisionByZero { remainder },
                );
            } else {
                let ProgramOutcome::Returned(value) = report.outcome else {
                    causes::unexpected(&mut heap, report.outcome, "empty range return")
                };
                assert_ints(&heap, &value, &[]);
                heap.drop_value(value).expect("result release");
            }
            heap.finish().expect("every execution root released");
        }
    }
}
#[test]
fn partial_calls_and_source_closures_preserve_heap_capture_and_reference_identity() {
    use flick_model::memory::{ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "captured", release);
        for partial in [false, true] {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("kept".into()))
                .expect("external capture");
            let input = heap.dup(&external).expect("capture alias");
            let mut arguments = vec![input];
            if !partial {
                arguments.push(Value::Unit);
            }
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Range", "captured"),
                arguments,
                100_000,
            );
            let ProgramOutcome::Returned(value) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "cold closure or list result")
            };
            if partial {
                assert_eq!(
                    heap.reference_count(&external),
                    Ok(2),
                    "returned cold closure retains actual capture"
                );
            } else {
                assert_ints(&heap, &value, &[4, 5]);
            }
            heap.drop_value(value)
                .expect("returned closure/list release");
            assert_eq!(heap.string(&external), Ok("kept"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            heap.drop_value(external).expect("external release");
            heap.finish().expect("actual zero occupied slots");
        }
        let p = program(CASES, "identity", release);
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &p,
            &Name::new("App.Range", "identity"),
            vec![Value::Unit],
            100_000,
        )
        .expect("Ref sources remain ordinary caller values");
        assert_eq!(ran.value, Value::Bool(true));
        heap.drop_value(ran.value).expect("result release");
        heap.finish().expect("references released");
    }
}
#[test]
fn range_composes_with_inert_closures_holding_retired_arena_views_at_every_prefix() {
    use flick_model::memory::{Error, ProgramOutcome, execute_program_outcome};
    for release in [false, true] {
        let p = program(CASES, "holding", release);
        let mut completed = false;
        for fuel in 0..=8192 {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("outside".into()))
                .expect("external root");
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Range", "holding"),
                vec![Value::Unit],
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_ints(&heap, &value, &[0, 1]);
                    heap.drop_value(value).expect("range release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => {
                    let detail = format!("{other:?}");
                    other.release(&mut heap).expect("release");
                    panic!("{detail}")
                }
            }
            assert_eq!(report.stdout, "");
            assert_eq!(heap.string(&outside), Ok("outside"));
            heap.drop_value(outside).expect("external release");
            let stats = heap
                .finish()
                .expect("all retired arena and execution storage released");
            assert_eq!(stats.allocations, stats.frees);
            assert_eq!(stats.arenas_allocated, stats.arenas_freed);
            if completed {
                assert_eq!(stats.arenas_allocated, 1);
                eprintln!("range RG4 release={release} terminal={fuel}");
                break;
            }
        }
        assert!(completed);
    }
}

#[test]
fn genuine_partial_range_is_cold_until_the_second_endpoint() {
    for release in [false, true] {
        let p = program(CASES, "cold", release);
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &p,
            &Name::new("App.Range", "cold"),
            vec![Value::Unit],
            100_000,
        )
        .expect("partial minimum endpoint returns a function, not billions of elements");
        assert_eq!(ran.stdout, "");
        assert_ne!(heap.tag(&ran.value), Ok("$list"));
        heap.drop_value(ran.value)
            .expect("cold partial closure release");
        heap.finish().expect("zero occupied slots");
    }
}
#[test]
fn identical_data_abi_requires_actual_int_inputs_and_exact_list_int_output() {
    use flick_core::ir::ItemKind;
    for (input, second, output) in [
        ("Float", "Int", "List Int"),
        ("Int", "Float", "List Int"),
        ("Float", "Float", "List Int"),
        ("String", "String", "List Int"),
        ("Int", "Int", "List Float"),
        ("Int", "Int", "List String"),
        ("Int", "Int", "List a"),
    ] {
        let source = format!(
            "module App.Range exposing (custom)\ncustom : {input} -> {second} -> {output}\nforeign pure custom from \"flick:rt\" \"listRange\"\n"
        );
        let modules =
            run::compile_source(&source).expect("real checked non-Int foreign declaration");
        let original = modules
            .iter()
            .flat_map(|m| &m.ir.items)
            .find_map(|i| {
                if let ItemKind::Foreign(f) = &i.kind {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("genuine raw foreign");
        assert_eq!(
            serde_json::to_value(&original.adapter.shapes).expect("shape metadata"),
            serde_json::json!(["Data", "Data"])
        );
        assert_eq!(original.adapter.copies.args, [false, false]);
        assert!(original.adapter.copies.result);
        for release in [false, true] {
            let ir: Vec<_> = modules
                .iter()
                .map(|m| {
                    if release {
                        build::optimized(&modules, m, false)
                    } else {
                        m.ir.clone()
                    }
                })
                .collect();
            let error = perceus::lower_with_declarations(
                ir.iter().chain(build::builtin_modules()),
                modules
                    .iter()
                    .map(|m| m.declarations.as_ref())
                    .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
                &Name::new("App.Range", "custom"),
            )
            .expect_err("same ABI is not an Int proof");
            assert!(
                error.to_string().contains("List.range:"),
                "{input}->{second}->{output}: {error}"
            );
        }
    }
}

#[test]
fn checked_alias_inputs_and_alias_list_output_execute_through_the_public_context_api() {
    let source = r#"module App.Range exposing (alias)
type alias Endpoint = Int
type alias Values a = List a
custom : Endpoint -> Endpoint -> Values Endpoint
foreign pure custom from "flick:rt" "listRange"
alias : Unit -> List Int
let alias () = custom 1 3
"#;
    for release in [false, true] {
        let p = program(source, "alias", release);
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &p,
            &Name::new("App.Range", "alias"),
            vec![Value::Unit],
            100_000,
        )
        .expect("genuine checked transparent aliases");
        assert_ints(&heap, &ran.value, &[1, 2]);
        heap.drop_value(ran.value).expect("result release");
        heap.finish().expect("actual zero occupied slots");
    }
}
