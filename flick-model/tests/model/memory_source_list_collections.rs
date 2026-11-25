//! Pure collection callbacks stay inside owned Flick code, in source order.
use crate::causes;
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, Value, execute_program, execute_program_outcome},
    run,
};

const SOURCE: &str = r#"module App.Collections exposing (answer, boundary, cold, live, nominal, escaped, fault_map, fault_equal, fault_compare, fault_hash)
map_list : (a -> b) -> List a -> List b
foreign pure map_list from "flick:rt" "listMap"
equal_list : (a -> a -> Bool) -> List a -> List a -> Bool
foreign pure equal_list from "flick:rt" "listEquals"
compare_list : (a -> a -> Ordering) -> List a -> List a -> Ordering
foreign pure compare_list from "flick:rt" "listCompare"
hash_list : (a -> Int) -> List a -> Int
foreign pure hash_list from "flick:rt" "listHash"
render : Ordering -> String
let render x = match x with | LT -> "LT" | EQ -> "EQ" | GT -> "GT"
answer : Unit -> String
let answer () =
  let prefix = "v"
  let mapper = map_list (fun x -> prefix ++ x)
  show (mapper ["a", "b"]) ++ "|" ++ show (equal_list (fun _ _ -> true) [1, 2] [8, 9])
    ++ "|" ++ render (compare_list (fun _ _ -> GT) [1] [2])
    ++ "|" ++ show (hash_list (fun x -> x) [1, 2, 3])

type Token = Token Bool String
instance Eq Token =
  let eq (Token left _) (Token right _) = left == right
instance Ord Token =
  let compare (Token left _) (Token right _) = if left == right then EQ else if left then GT else LT
instance Hash Token =
  let hash (Token flag _) = if flag then 17 else 9
live : Unit -> String
let live () =
  let a = [Token true "a", Token false "b"]
  let b = [Token true "changed", Token false "labels"]
  show (List.map (fun x -> x ++ "!") ["a", "b"])
    ++ "|" ++ show (map (fun x -> x + 1) [2, 3])
    ++ "|" ++ show (eq a b) ++ "|" ++ render (compare a b)
    ++ "|" ++ show (hash a)

nominal : NonEmpty.NonEmpty String -> NonEmpty.NonEmpty String
let nominal values = NonEmpty.map (fun value -> value ++ "!") values

spin : Unit -> a
let rec spin () = spin ()
trap_int : Int -> Int
let trap_int _ = spin ()
trap_equal : Int -> Int -> Bool
let trap_equal _ _ = spin ()
trap_order : Int -> Int -> Ordering
let trap_order _ _ = spin ()
boundary : Unit -> String
let boundary () =
  show (map_list trap_int []) ++ "|" ++ show (hash_list trap_int [])
    ++ "|" ++ show (equal_list trap_equal [1] [2, 3])
    ++ "|" ++ show (equal_list trap_equal [] [])
    ++ "|" ++ show (equal_list (fun x _ -> if x == 1 then false else spin ()) [1, 2] [9, 9])
    ++ "|" ++ render (compare_list (fun x _ -> if x == 1 then GT else spin ()) [1, 2] [9, 9])
    ++ "|" ++ render (compare_list trap_order [] [1])
    ++ "|" ++ render (compare_list trap_order [1] [])
    ++ "|" ++ render (compare_list trap_order [] [])
    ++ "|" ++ render (compare_list (fun _ _ -> EQ) [8] [1, 2])
    ++ "|" ++ render (compare_list (fun _ _ -> EQ) [8, 9] [1])
    ++ "|" ++ show (hash_list (fun x -> x) [2147483647, 2147483647, -2147483648])
cold : Unit -> String
let cold () =
  let mapper = map_list trap_int
  let eq_first = equal_list trap_equal [1]
  let cmp_first = compare_list trap_order [1]
  let hasher = hash_list trap_int
  "cold"
escaped : String -> String
let escaped prefix =
  let mapped = map_list (fun value -> fun suffix -> prefix ++ value ++ suffix) ["a", "b"]
  match mapped with
  | [first, second] -> first "!" ++ "|" ++ second "?"
  | _ -> "bad"
fault_map : String -> String
let fault_map prefix = show (map_list (fun value -> if value == 0 then prefix ++ ":first" else spin ()) [0, 1]) ++ prefix
fault_equal : String -> String
let fault_equal prefix = show (equal_list (fun value _ -> if value == 0 then prefix == "outside" else spin ()) [0, 1] [0, 1]) ++ prefix
fault_compare : String -> String
let fault_compare prefix = render (compare_list (fun value _ -> if value == 0 && prefix == "outside" then EQ else spin ()) [0, 1] [0, 1]) ++ prefix
fault_hash : String -> String
let fault_hash prefix = show (hash_list (fun value -> if value == 0 then (if prefix == "outside" then 1 else 0) else spin ()) [0, 1]) ++ prefix
"#;

#[test]
fn checked_collections_use_callbacks_not_payloads() {
    let modules = run::compile_source(SOURCE).expect("checked collection source");
    let entry = Name::new("App.Collections", "answer");
    for release in [false, true] {
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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("pure collection adapters");
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("collection execution");
        assert_eq!(
            heap.string(&ran.value),
            Ok("[\"va\", \"vb\"]|true|GT|209563"),
            "release={release}"
        );
        assert_eq!(ran.stdout, "");
        heap.drop_value(ran.value).expect("release result");
        let memory = heap.finish().expect("all roots released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

fn program(name: &str, release: bool) -> (Program, Name) {
    let modules = run::compile_source(SOURCE).expect("collection source checks");
    let entry = Name::new("App.Collections", name);
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
    let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
        .expect("closed collection templates");
    (program, entry)
}

#[test]
fn empty_short_circuit_lengths_overflow_and_partial_application_are_exact() {
    for (name, expected) in [
        (
            "boundary",
            "[]|7|false|true|false|GT|LT|GT|EQ|LT|GT|-2147276103",
        ),
        ("cold", "cold"),
        ("live", "[\"a!\", \"b!\"]|[3, 4]|true|EQ|7263"),
    ] {
        for release in [false, true] {
            let (program, entry) = program(name, release);
            let mut heap = Heap::new();
            let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
                .expect("no skipped callback is evaluated");
            assert_eq!(
                heap.string(&ran.value),
                Ok(expected),
                "{name} release={release}"
            );
            heap.drop_value(ran.value).expect("result release");
            let stats = heap.finish().expect("actual zero roots");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn every_prefix_through_success_preserves_shared_callback_captures() {
    for (name, expected) in [
        ("escaped", "prefix:a!|prefix:b?"),
        ("answer", "[\"va\", \"vb\"]|true|GT|209563"),
        ("live", "[\"a!\", \"b!\"]|[3, 4]|true|EQ|7263"),
    ] {
        for release in [false, true] {
            let (program, entry) = program(name, release);
            let mut completed = false;
            for fuel in 0..=8192 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("prefix:".into()))
                    .expect("external root");
                let input = if name == "escaped" {
                    heap.dup(&external)
                        .expect("actual alias passed to callback captures")
                } else {
                    Value::Unit
                };
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        assert_eq!(heap.string(&value), Ok(expected));
                        heap.drop_value(value).expect("returned closure result");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => {
                        let diagnostic = format!("release={release} fuel={fuel}: {other:?}");
                        causes::unexpected(&mut heap, other, &diagnostic)
                    }
                }
                assert_eq!(heap.string(&external), Ok("prefix:"));
                assert_eq!(
                    heap.stats().live_objects,
                    1,
                    "release={release} fuel={fuel}"
                );
                heap.drop_value(external).expect("external alias release");
                let stats = heap.finish().expect("no hidden cleanup or sweep");
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!("{name} release={release}: all {} prefixes", fuel + 1);
                    break;
                }
            }
            assert!(completed, "prefix range reaches successful execution");
        }
    }
}

#[test]
fn callback_fault_prefixes_release_preceding_results_and_shared_captures() {
    for name in ["fault_map", "fault_equal", "fault_compare", "fault_hash"] {
        for release in [false, true] {
            let (program, entry) = program(name, release);
            for fuel in 0..=512 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("caller root");
                let input = heap.dup(&external).expect("shared input");
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
                {
                    let diagnostic = format!("{name} release={release} fuel={fuel}: {report:?}");
                    let observed_outcome = report.outcome;
                    let correct =
                        matches!(&observed_outcome, ProgramOutcome::Fault(Error::OutOfFuel));
                    let detail = format!("{observed_outcome:?}");
                    observed_outcome
                        .release(&mut heap)
                        .expect("every owned outcome released before assertion");
                    assert!(correct, "{diagnostic}: {detail}");
                };
                assert_eq!(heap.string(&external), Ok("outside"));
                assert_eq!(
                    heap.stats().live_objects,
                    1,
                    "{name} release={release} fuel={fuel}"
                );
                heap.drop_value(external).expect("external release");
                let stats = heap
                    .finish()
                    .expect("actual zero roots after callback fault");
                assert_eq!(stats.allocations, stats.frees);
            }
        }
    }
}

#[test]
fn nonempty_canonical_map_keeps_nominal_list_representation_and_shared_elements() {
    for release in [false, true] {
        let (program, entry) = program("nominal", release);
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("x".into()))
            .expect("external element");
        let left = heap.dup(&external).expect("first alias");
        let right = heap.dup(&external).expect("second alias");
        let input = heap
            .construct("$list", vec![left, right])
            .expect("NonEmpty uses the list representation");
        let ran = execute_program(&mut heap, &program, &entry, vec![input], 100_000)
            .expect("canonical NonEmpty callback adapter");
        assert_eq!(heap.tag(&ran.value), Ok("$list"));
        for index in 0..2 {
            assert_eq!(
                heap.string(heap.field(&ran.value, index).expect("mapped element")),
                Ok("x!")
            );
        }
        heap.drop_value(ran.value).expect("mapped list release");
        assert_eq!(heap.string(&external), Ok("x"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("external root release");
        let stats = heap.finish().expect("actual zero roots");
        assert_eq!(stats.allocations, stats.frees);
    }
}
