//! Checked source exercises genuine cold language-value adapters.
use crate::causes;
use flick_core::{
    build,
    ir::{
        ItemKind, Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
const DECLARATIONS: &str = r#"flat_map_list : (forall v. a^v -> (List b)^(c + v))^c -> (List a)^d -> (List b)^(c + d)
foreign pure flat_map_list from "flick:rt" "listFlatMap"
  arity 2
  retention call
"#;
fn program(body: &str, name: &str, release: bool) -> Program {
    let source = format!("module App.Adapters exposing (..)\n{DECLARATIONS}\n{body}");
    let modules = run::compile_source(&source).expect("genuine checked source");
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
    perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        &Name::new("App.Adapters", name),
    )
    .expect("closed adapters lower")
}
const CASES: &str = r#"
rows : String -> List String
let rows text = flat_map_list (fun value -> if value == "skip" then [] else [text ++ value, value, value]) ["a", "skip", "b"]
spin : Unit -> a
let rec spin () = spin ()
cold : String -> List String
let cold seed =
  let _partial = flat_map_list (fun value -> [seed ++ value ++ spin ()])
  flat_map_list (fun _ -> spin ()) []
escaped : String -> String
let escaped prefix =
  let values = flat_map_list (fun item -> [fun suffix -> prefix ++ item ++ suffix]) ["a", "b"]
  match values with | [first, second] -> first "!" ++ "|" ++ second "?" | _ -> "bad"
order : Int -> List Int
let order target = flat_map_list (fun value -> if value < target then [value] else if value == target then [Int.unsafe_div 1 0] else [Int.unsafe_rem 1 0]) [0, 1, 2]
identity : Unit ->{} Bool
let identity () =
  let cell = Ref.make "kept"
  let values = flat_map_list (fun value -> [value, value]) [cell]
  match values with
  | [first, second] ->
    Ref.set first "changed"
    Ref.same first cell && Ref.same second cell && Ref.get cell == "changed"
  | _ -> false
argument : String -> List String ->{} List String
let argument label values =
  Host.raw_stdout_line label
  values
factory : Unit -> (String -> List String)
let factory () = fun value -> [value, value]
eager : Unit ->{} List String
let eager () = flat_map_list (factory ()) (argument "input" ["x", "y"])
factory_bad : Unit -> (String -> List String)
let factory_bad () = if Int.unsafe_div 1 0 == 0 then fun x -> [x] else fun _ -> []
eager_fault : Unit ->{} List String
let eager_fault () = flat_map_list (factory_bad ()) (argument "unreachable" [])
"#;
fn assert_strings(heap: &Heap, value: &Value, expected: &[&str]) {
    assert_eq!(heap.tag(value), Ok("$list"));
    for (index, text) in expected.iter().enumerate() {
        assert_eq!(
            heap.string(heap.field(value, index).expect("ordered item")),
            Ok(*text)
        );
    }
    assert!(heap.field(value, expected.len()).is_err());
}
#[test]
fn rows_order_multiplicity_empty_partial_and_reference_aliases() {
    for release in [false, true] {
        for (name, input, expected, stdout) in [
            ("rows", true, vec!["kepta", "a", "a", "keptb", "b", "b"], ""),
            ("cold", true, vec![], ""),
            ("eager", false, vec!["x", "x", "y", "y"], "input\n"),
        ] {
            let p = program(CASES, name, release);
            let mut heap = Heap::new();
            let input = if input {
                heap.literal(&Lit::String("kept".into())).expect("capture")
            } else {
                Value::Unit
            };
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", name),
                vec![input],
                100_000,
            );
            let ProgramOutcome::Returned(value) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "list return")
            };
            assert_strings(&heap, &value, &expected);
            assert_eq!(report.stdout, stdout);
            heap.drop_value(value).expect("return release");
            heap.finish().expect("all roots released");
        }
        let p = program(CASES, "identity", release);
        let mut heap = Heap::new();
        let report = execute_program_outcome(
            &mut heap,
            &p,
            &Name::new("App.Adapters", "identity"),
            vec![Value::Unit],
            100_000,
        );
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "identity")
        };
        assert_eq!(value, Value::Bool(true));
        heap.drop_value(value).expect("result release");
        heap.finish().expect("refs released");
    }
}
#[test]
fn first_callback_and_eager_function_defects_have_exact_order() {
    for release in [false, true] {
        for target in 0..3 {
            let p = program(CASES, "order", release);
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", "order"),
                vec![Value::Int(target)],
                100_000,
            );
            assert_eq!(report.stdout, "");
            causes::assert_defect(
                &mut heap,
                report.outcome,
                &Error::IntDivisionByZero { remainder: false },
            );
            heap.finish()
                .expect("completed rows released before later fault");
        }
        let p = program(CASES, "eager_fault", release);
        let mut heap = Heap::new();
        let report = execute_program_outcome(
            &mut heap,
            &p,
            &Name::new("App.Adapters", "eager_fault"),
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
            .expect("function fails before input expression");
    }
}
#[test]
fn every_prefix_preserves_external_aliases_and_escaped_callback_outputs() {
    for release in [false, true] {
        for (name, expected) in [
            ("rows", None),
            ("cold", None),
            ("escaped", Some("kepta!|keptb?")),
        ] {
            let p = program(CASES, name, release);
            let mut completed = false;
            for fuel in 0..=8192 {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("kept".into()))
                    .expect("external root");
                let input = heap.dup(&outside).expect("capture alias");
                let report = execute_program_outcome(
                    &mut heap,
                    &p,
                    &Name::new("App.Adapters", name),
                    vec![input],
                    fuel,
                );
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        if let Some(expected) = expected {
                            assert_eq!(heap.string(&value), Ok(expected));
                        } else if name == "rows" {
                            assert_strings(&heap, &value, &["kepta", "a", "a", "keptb", "b", "b"]);
                        } else {
                            assert_strings(&heap, &value, &[]);
                        }
                        heap.drop_value(value).expect("return release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => causes::unexpected(&mut heap, other, "prefix outcome"),
                }
                assert_eq!(heap.string(&outside), Ok("kept"));
                assert_eq!(heap.reference_count(&outside), Ok(1));
                heap.drop_value(outside).expect("caller release");
                let stats = heap.finish().expect("actual zero occupied slots");
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!("flatmap {name} release={release} terminal={fuel}");
                    break;
                }
            }
            assert!(completed);
        }
    }
}
const TUPLES: &str = r#"
opaque type alias Pair a = (a, a)
tuple_rows : (forall v. a^v -> (Pair b)^(c + v))^c -> (a, a)^d -> (List b)^(c + d)
foreign pure tuple_rows from "flick:rt" "listFlatMap"
  arity 2
  retention call
tuple_aliases : String -> List String
let tuple_aliases seed = tuple_rows (fun value -> ((value, value) : Pair String)) (seed, seed)
"#;
#[test]
fn tuple_input_and_opaque_tuple_rows_share_the_exact_readonly_array_adapter() {
    let text = format!("module App.Adapters exposing (..)\n{DECLARATIONS}\n{TUPLES}");
    let modules = run::compile_source(&text).expect("accepted genuine tuple ABI");
    let foreign = modules
        .iter()
        .flat_map(|m| &m.ir.items)
        .find_map(|i| {
            if let ItemKind::Foreign(f) = &i.kind
                && f.name == "tuple_rows"
            {
                Some(f)
            } else {
                None
            }
        })
        .expect("tuple foreign");
    let genuine = build::builtin_modules()
        .iter()
        .flat_map(|m| &m.items)
        .find_map(|i| {
            if let ItemKind::Foreign(f) = &i.kind
                && f.name == "flat_map_list"
            {
                Some(f)
            } else {
                None
            }
        })
        .expect("genuine bridge");
    assert_eq!(
        foreign.adapter, genuine.adapter,
        "ONLY the exact audited ABI exercises normalization"
    );
    for release in [false, true] {
        let p = program(TUPLES, "tuple_aliases", release);
        let mut completed = false;
        for fuel in 0..=4096 {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("outside".into()))
                .expect("external");
            let argument = heap.dup(&outside).expect("input alias");
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", "tuple_aliases"),
                vec![argument],
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_strings(&heap, &value, &["outside", "outside", "outside", "outside"]);
                    for index in 0..4 {
                        assert_eq!(
                            heap.field(&value, index).expect("alias").object_id(),
                            outside.object_id()
                        );
                    }
                    heap.drop_value(value).expect("result release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => causes::unexpected(&mut heap, other, "tuple prefix"),
            }
            assert_eq!(heap.string(&outside), Ok("outside"));
            assert_eq!(heap.reference_count(&outside), Ok(1));
            heap.drop_value(outside).expect("external release");
            heap.finish().expect("actual zero occupied slots");
            if completed {
                eprintln!("flatmap tuples release={release} terminal={fuel}");
                break;
            }
        }
        assert!(completed);
    }
}
const HOLDING: &str = r#"
import Arena
holding : Unit ->{} Int
let holding () =
  let retained = scope s {
    let view = Arena.alloc (Arena.make ()) "retired"
    [fun () -> ignore view, fun () -> ignore view]
  }
  List.length (flat_map_list (fun value -> [value, value]) retained)
"#;
#[test]
fn flattening_does_not_invoke_closures_holding_retired_arena_views() {
    for release in [false, true] {
        let p = program(HOLDING, "holding", release);
        let mut completed = false;
        for fuel in 0..=4096 {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("outside".into()))
                .expect("external");
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", "holding"),
                vec![Value::Unit],
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(value, Value::Int(4));
                    heap.drop_value(value).expect("result release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => causes::unexpected(&mut heap, other, "retired arena prefix"),
            }
            assert_eq!(heap.string(&outside), Ok("outside"));
            heap.drop_value(outside).expect("external release");
            let stats = heap.finish().expect("retired arena and all roots released");
            assert_eq!(stats.arenas_allocated, stats.arenas_freed);
            if completed {
                assert_eq!(stats.arenas_allocated, 1);
                eprintln!("flatmap RG4 release={release} terminal={fuel}");
                break;
            }
        }
        assert!(completed);
    }
}
const MONO: &str = r#"
rows : String -> List String
let rows text = List.flat_map (fun value -> if value == "skip" then [] else [text ++ value, value, value]) ["a", "skip", "b"]
escaped : String -> String
let escaped prefix =
  let values = List.flat_map (fun item -> [fun suffix -> prefix ++ item ++ suffix]) ["a", "b"]
  match values with | [first, second] -> first "!" ++ "|" ++ second "?" | _ -> "bad"
order : Int -> List Int
let order target = List.flat_map (fun value -> if value < target then [value] else if value == target then [Int.unsafe_div 1 0] else [Int.unsafe_rem 1 0]) [0, 1, 2]
tuple_mono : (a -> (b,b))^c -> (a,a)^d -> (List b)^(c+d)
foreign pure tuple_mono from "flick:rt" "listFlatMap"
  arity 2
  retention call
tuple_aliases : String -> List String
let tuple_aliases seed = tuple_mono (fun value -> (value,value)) (seed,seed)
"#;
#[test]
fn genuine_public_monomorphic_flat_map_preserves_rows_order_closures_and_array_aliases() {
    for release in [false, true] {
        for name in ["rows", "escaped", "tuple_aliases"] {
            let p = program(MONO, name, release);
            let mut completed = false;
            for fuel in 0..=8192 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("kept".into()))
                    .expect("external root");
                let input = heap.dup(&external).expect("input alias");
                let report = execute_program_outcome(
                    &mut heap,
                    &p,
                    &Name::new("App.Adapters", name),
                    vec![input],
                    fuel,
                );
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        if name == "escaped" {
                            assert_eq!(heap.string(&value), Ok("kepta!|keptb?"));
                        } else if name == "rows" {
                            assert_strings(&heap, &value, &["kepta", "a", "a", "keptb", "b", "b"]);
                        } else {
                            assert_strings(&heap, &value, &["kept", "kept", "kept", "kept"]);
                            for index in 0..4 {
                                assert_eq!(
                                    heap.field(&value, index).expect("alias").object_id(),
                                    external.object_id()
                                );
                            }
                        }
                        heap.drop_value(value).expect("return release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => causes::unexpected(&mut heap, other, "monomorphic prefix"),
                }
                assert_eq!(heap.string(&external), Ok("kept"));
                assert_eq!(heap.reference_count(&external), Ok(1));
                heap.drop_value(external).expect("external release");
                heap.finish().expect("actual zero occupied heap");
                if completed {
                    eprintln!("flatmap mono {name} release={release} terminal={fuel}");
                    break;
                }
            }
            assert!(completed);
        }
        for target in 0..3 {
            let p = program(MONO, "order", release);
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", "order"),
                vec![Value::Int(target)],
                100_000,
            );
            causes::assert_defect(
                &mut heap,
                report.outcome,
                &Error::IntDivisionByZero { remainder: false },
            );
            heap.finish().expect("ordered callback releases prior rows");
        }
    }
}

#[test]
fn copied_declarations_match_the_entire_genuine_builtin_foreign_abi() {
    let source = format!("module App.Adapters exposing (..)\n{DECLARATIONS}");
    let modules = run::compile_source(&source).expect("genuine copied declarations");
    let mut count = 0;
    for item in modules.iter().flat_map(|m| &m.ir.items) {
        let ItemKind::Foreign(actual) = &item.kind else {
            continue;
        };
        let expected = build::builtin_modules()
            .iter()
            .flat_map(|m| &m.items)
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(value)
                    if value.name == actual.name && value.binding == actual.binding =>
                {
                    Some(value)
                }
                _ => None,
            })
            .expect("real corresponding builtin");
        assert_eq!(
            serde_json::to_value(actual).expect("actual ABI"),
            serde_json::to_value(expected).expect("genuine ABI")
        );
        count += 1;
    }
    assert_eq!(count, 1);
}
