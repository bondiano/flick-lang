//! List length does not evaluate elements; `any` visits left-to-right and
//! stops at its first true callback. Every tested prefix releases real roots.
use crate::causes;
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::run;
use flick_vm::tier0::{
    Error, Heap, ProgramOutcome, Value, execute_main_program_outcome, execute_program_outcome,
};

const FUEL: u64 = 100_000;
const SOURCE: &str = r#"module App.ListPredicates exposing (lengths, choices, cold, nominal, fault)
spin : Unit -> a
let rec spin () = spin ()

lengths : String -> String
let lengths seed =
  let values = [seed, seed, seed]
  let saved = values
  let functions = [fun () -> spin (), fun () -> spin ()]
  show (List.length ([] : List Int)) ++ "|" ++ show (List.length values)
    ++ "|" ++ show (List.length functions) ++ "|" ++ show (List.length saved)
    ++ "|" ++ seed

choices : String -> String
let choices seed =
  let predicate value =
    if value == "hit" then seed == "outside"
    else if value == "bad" then Int.unsafe_div 1 0 == 0
    else false
  show (List.any predicate ["miss", "hit", "bad"])
    ++ "|" ++ show (List.any predicate ["miss"])
    ++ "|" ++ show (List.any (fun _ -> spin ()) []) ++ "|" ++ seed

cold : String -> String
let cold seed =
  let captured = seed ++ ":held"
  let _unused = List.any (fun _ -> if captured == "" then true else spin ())
  seed

nominal : NonEmpty.NonEmpty String -> Int
let nominal values = NonEmpty.length values

fault : String -> Bool
let fault seed =
  let captured = seed ++ ":held"
  List.any
    (fun value ->
      if captured == "" then false
      else if value == 0 then Int.unsafe_div 1 0 == 0
      else spin ()
    )
    [0, 1]
"#;

const HOLDING_SOURCE: &str = r#"module App.ListPredicates exposing (holding, tuple_holding)
import Arena
pair_length : (a, b) -> Int
foreign pure pair_length from "flick:rt" "listLength"
pair_any : (a -> Bool) -> (a, a) -> Bool
foreign pure pair_any from "flick:rt" "listAny"
  arity 2

holding : Unit ->{} Int
let holding () =
  let retained = scope s {
    let view = Arena.alloc (Arena.make ()) "retired"
    [fun () -> ignore view, fun () -> ignore view]
  }
  if List.any (fun _ -> true) retained then List.length retained else 0

tuple_holding : Unit ->{} Int
let tuple_holding () =
  let retained = scope s {
    let view = Arena.alloc (Arena.make ()) "retired"
    (fun () -> ignore view, fun () -> ignore view)
  }
  if pair_any (fun _ -> true) retained then pair_length retained else 0
"#;

const TUPLE_SOURCE: &str = r#"module App.ListPredicates exposing (tuple_lengths, tuple_choices, tuple_nominal, tuple_cold, tuple_fault)
pair_length : (a, b) -> Int
foreign pure pair_length from "flick:rt" "listLength"
pair_any : (a -> Bool) -> (a, a) -> Bool
foreign pure pair_any from "flick:rt" "listAny"
  arity 2
opaque type alias Pair = (String, String)
nominal_length : Pair -> Int
foreign pure nominal_length from "flick:rt" "listLength"
spin : Unit -> a
let rec spin () = spin ()

tuple_lengths : String -> String
let tuple_lengths seed =
  let values = (seed, seed)
  let saved = values
  show (pair_length values) ++ "|" ++ show (pair_length (fun () -> spin (), fun () -> spin ()))
    ++ "|" ++ show (pair_length saved) ++ "|" ++ seed

tuple_choices : String -> String
let tuple_choices seed =
  let predicate value =
    if value == "hit" then seed == "outside"
    else if value == "bad" then Int.unsafe_div 1 0 == 0
    else false
  show (pair_any predicate ("hit", "bad"))
    ++ "|" ++ show (pair_any predicate ("miss", "hit"))
    ++ "|" ++ show (pair_any predicate ("miss", "miss")) ++ "|" ++ seed

tuple_nominal : String -> String
let tuple_nominal seed =
  let values : Pair = (seed, seed)
  show (nominal_length values) ++ "|" ++ seed

tuple_cold : String -> String
let tuple_cold seed =
  let _unused = pair_any (fun _ -> if seed == "" then true else spin ())
  seed

tuple_fault : String -> Bool
let tuple_fault seed =
  pair_any
    (fun value -> if seed == "" then false else if value == 0 then Int.unsafe_div 1 0 == 0 else spin ())
    (0, 1)
"#;

fn programs(name: &str) -> (Name, [Program; 2]) {
    programs_from(SOURCE, name)
}

fn programs_from(source: &str, name: &str) -> (Name, [Program; 2]) {
    let modules = run::compile_source(source).expect("valid list source");
    let entry = Name::new("App.ListPredicates", name);
    let programs = [false, true].map(|release| {
        let ir: Vec<_> = modules
            .iter()
            .map(|module| {
                if release {
                    build::optimized(&modules, module, false)
                } else {
                    (*module.ir).clone()
                }
            })
            .collect();
        perceus::lower_with_declarations(
            ir.iter().chain(build::builtin_modules()),
            modules
                .iter()
                .map(|module| module.declarations.as_ref())
                .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
            &entry,
        )
        .expect("exact list length/any adapters lower")
    });
    (entry, programs)
}

fn expected(name: &str) -> &'static str {
    match name {
        "lengths" => "0|3|2|3|outside",
        "choices" => "true|false|false|outside",
        "cold" | "tuple_cold" => "outside",
        "tuple_lengths" => "2|2|2|outside",
        "tuple_choices" => "true|true|false|outside",
        "tuple_nominal" => "2|outside",
        _ => panic!("known source result"),
    }
}

fn check(name: &str) {
    check_from(SOURCE, name);
}

fn check_from(source: &str, name: &str) {
    let (entry, programs) = programs_from(source, name);
    for program in programs {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("outside".into()))
            .expect("caller root");
        let input = heap.dup(&outside).expect("owned argument");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], FUEL);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok(expected(name)));
        assert_eq!(report.stdout, "");
        heap.drop_value(value).expect("result release");
        assert_eq!(heap.string(&outside), Ok("outside"));
        assert_eq!(
            heap.stats().live_objects,
            1,
            "no execution-owned roots remain"
        );
        heap.drop_value(outside).expect("external release");
        let stats = heap.finish().expect("actual zero occupied slots");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn lengths_preserve_shared_lists_and_do_not_call_function_elements() {
    check("lengths");
}

#[test]
fn any_is_ordered_short_circuiting_empty_safe_and_cold_when_partial() {
    check("choices");
    check("cold");
}

#[test]
fn physical_tuple_arrays_and_their_nominal_aliases_follow_the_same_native_contract() {
    for name in [
        "tuple_lengths",
        "tuple_choices",
        "tuple_nominal",
        "tuple_cold",
    ] {
        check_from(TUPLE_SOURCE, name);
    }
}

#[test]
fn nonempty_length_preserves_the_original_list_and_shared_element_aliases() {
    let (entry, programs) = programs("nominal");
    for program in programs {
        let mut heap = Heap::new();
        let element = heap
            .literal(&Lit::String("shared".into()))
            .expect("element");
        let values = (0..3)
            .map(|_| heap.dup(&element).expect("list field"))
            .collect();
        let original = heap
            .construct("$list", values)
            .expect("NonEmpty representation");
        let input = heap.dup(&original).expect("independent list alias");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], FUEL);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(value, Value::Int(3));
        heap.drop_value(value).expect("immediate result");
        for index in 0..3 {
            assert_eq!(
                heap.string(heap.field(&original, index).expect("original field")),
                Ok("shared")
            );
        }
        assert_eq!(
            heap.stats().live_objects,
            2,
            "only original list and element"
        );
        heap.drop_value(original).expect("original list release");
        assert_eq!(heap.string(&element), Ok("shared"));
        heap.drop_value(element)
            .expect("independent element release");
        let stats = heap.finish().expect("no retained nominal list roots");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn every_prefix_through_result_or_first_callback_defect_releases_owned_roots() {
    let mut prefixes = 0;
    for (source, name) in [
        (SOURCE, "lengths"),
        (SOURCE, "choices"),
        (SOURCE, "cold"),
        (SOURCE, "fault"),
        (TUPLE_SOURCE, "tuple_lengths"),
        (TUPLE_SOURCE, "tuple_choices"),
        (TUPLE_SOURCE, "tuple_nominal"),
        (TUPLE_SOURCE, "tuple_cold"),
        (TUPLE_SOURCE, "tuple_fault"),
    ] {
        let (entry, programs) = programs_from(source, name);
        for (profile, program) in programs.into_iter().enumerate() {
            let mut completed = false;
            for fuel in 0..=FUEL {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("caller root");
                let input = heap.dup(&outside).expect("owned argument");
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
                match report.outcome {
                    ProgramOutcome::Returned(value) if !name.ends_with("fault") => {
                        assert_eq!(heap.string(&value), Ok(expected(name)));
                        heap.drop_value(value).expect("result release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    ProgramOutcome::Failed(cause) if name.ends_with("fault") => {
                        assert_eq!(
                            causes::defect_error(&mut heap, cause),
                            Error::IntDivisionByZero { remainder: false }
                        );
                        completed = true;
                    }
                    outcome => {
                        let diagnostic =
                            format!("{name} profile={profile} fuel={fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(report.stdout, "");
                assert_eq!(heap.string(&outside), Ok("outside"));
                assert_eq!(
                    heap.stats().live_objects,
                    1,
                    "{name} profile={profile} fuel={fuel}"
                );
                heap.drop_value(outside).expect("external release");
                let stats = heap
                    .finish()
                    .expect("all prefix roots released without sweep");
                assert_eq!(stats.allocations, stats.frees);
                prefixes += 1;
                if completed {
                    eprintln!("{name} profile={profile} first terminal fuel={fuel}");
                    break;
                }
            }
            assert!(completed, "{name} reaches its intended terminal outcome");
        }
    }
    eprintln!("checked {prefixes} consecutive fuel prefixes");
}

#[test]
fn observers_do_not_invoke_closures_holding_retired_arena_views() {
    let mut prefixes = 0;
    for name in ["holding", "tuple_holding"] {
        let (entry, programs) = programs_from(HOLDING_SOURCE, name);
        for (profile, program) in programs.into_iter().enumerate() {
            let mut completed = false;
            for fuel in 0..=FUEL {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let report = execute_main_program_outcome(&mut heap, &program, &entry, fuel);
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        assert_eq!(value, Value::Int(2));
                        heap.drop_value(value).expect("result release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    outcome => {
                        let diagnostic =
                            format!("{name} profile={profile} fuel={fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(report.stdout, "");
                assert_eq!(heap.string(&outside), Ok("outside"));
                assert_eq!(
                    heap.stats().live_objects,
                    1,
                    "only external storage remains"
                );
                heap.drop_value(outside).expect("caller releases its root");
                let stats = heap
                    .finish()
                    .expect("actual arena and execution roots retired");
                assert_eq!(stats.allocations, stats.frees);
                assert_eq!(stats.arenas_allocated, stats.arenas_freed);
                if completed {
                    assert_eq!(stats.arenas_allocated, 1);
                    eprintln!("{name} profile={profile} first terminal fuel={fuel}");
                }
                prefixes += 1;
                if completed {
                    break;
                }
            }
            assert!(completed, "retention-only observer reaches its result");
        }
    }
    eprintln!("checked {prefixes} retained-view fuel prefixes");
}
