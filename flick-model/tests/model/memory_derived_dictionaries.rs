//! Derived methods use ordinary payloads, lazy evidence and canonical display.
use crate::causes;
use flick_core::ir::{
    Lit,
    memory::{Name, Program},
};
use flick_core::{build, opt::perceus};
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
use flick_model::run;

const RECURSIVE: &str = r#"module App.Derived exposing (answer)
type Tree = Leaf | Node Tree Int Tree
  deriving Eq, Ord, Hash, Show

answer : Unit -> String
let answer () =
  let first = Node Leaf 7 Leaf
  let nested = Node first 8 Leaf
  let rank =
    match compare first Leaf with
    | LT -> "LT"
    | EQ -> "EQ"
    | GT -> "GT"
  show nested ++ "|" ++ show (eq first (Node Leaf 7 Leaf)) ++ "|" ++ rank ++ "|" ++ show (hash first)
"#;

#[test]
fn recursive_derivation_keeps_constructor_order_hash_and_parentheses() {
    let text = RECURSIVE;
    let modules = run::compile_source(text).expect("derived recursive source checks");
    let entry = Name::new("App.Derived", "answer");
    for release in [false, true] {
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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("counted derived methods");
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(
            heap.string(&value),
            Ok("Node (Node Leaf 7 Leaf) 8 Leaf|true|GT|30008")
        );
        heap.drop_value(value).expect("result release");
        let memory = heap
            .finish()
            .expect("recursive dictionaries never cache self cycles");
        assert_eq!(memory.allocations, memory.frees);
    }
}

fn program(text: &str, module: &str, name: &str, release: bool) -> (Program, Name) {
    let modules = run::compile_source(text).expect("derived source checks");
    let entry = Name::new(module, name);
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
    let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
        .expect("derived ownership lowering");
    (program, entry)
}

const LAZY: &str = r"module App.LazyDerived exposing (empty, different, empty_order, different_order, bad)
type Bomb a = Bomb a

hang : Unit -> a
let rec hang () = hang ()

instance Eq a => Eq (Bomb a) =
  let eq = hang ()

instance Ord a => Ord (Bomb a) =
  let compare = hang ()

type Carrier a = Empty | Full (Bomb a)
  deriving Eq, Ord

empty : Unit -> Bool
let empty () = eq (Empty : Carrier Int) Empty

different : Unit -> Bool
let different () = not (eq (Empty : Carrier Int) (Full (Bomb 1)))

empty_order : Unit -> Bool
let empty_order () =
  match compare (Empty : Carrier Int) Empty with
  | EQ -> true
  | _ -> false

different_order : Unit -> Bool
let different_order () =
  match compare (Empty : Carrier Int) (Full (Bomb 1)) with
  | LT -> true
  | _ -> false

bad : Unit -> Bool
let bad () = eq (Full (Bomb (1 : Int))) (Full (Bomb 1))
";

#[test]
fn nullary_and_mismatched_variants_do_not_force_field_instance_factories() {
    for name in ["empty", "different", "empty_order", "different_order"] {
        for release in [false, true] {
            let (program, entry) = program(LAZY, "App.LazyDerived", name, release);
            let mut heap = Heap::new();
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
            let ProgramOutcome::Returned(value) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
            };
            assert_eq!(value, Value::Bool(true));
            heap.drop_value(value).expect("result release");
            let memory = heap.finish().expect("unused instance factories stay cold");
            assert_eq!(memory.allocations, memory.frees);
        }
    }
}

#[test]
fn forcing_a_field_factory_and_every_interrupted_prefix_release_actual_roots() {
    for release in [false, true] {
        let (program, entry) = program(LAZY, "App.LazyDerived", "bad", release);
        for fuel in 0..=512 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("outside".into()))
                .expect("caller root");
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], fuel);
            {
                let diagnostic = format!("release={release} fuel={fuel}: {report:?}");
                let observed_outcome = report.outcome;
                let correct = matches!(&observed_outcome, ProgramOutcome::Fault(Error::OutOfFuel));
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
                "release={release} fuel={fuel}"
            );
            heap.drop_value(external).expect("caller root release");
            let memory = heap
                .finish()
                .expect("no partial dictionary or recursive frame roots");
            assert_eq!(memory.allocations, memory.frees);
        }
    }
}

#[test]
fn opaque_representation_derivation_forwards_without_constructor_hash_or_display() {
    let text = r#"module App.LazyDerived exposing (answer)
opaque type alias Pair = (Int, String)
  deriving Eq, Ord, Hash, Show
answer : Unit -> String
let answer () =
  let value : Pair = (7, "x")
  show value ++ "|" ++ show (hash value) ++ "|" ++ show (eq value value)
"#;
    for release in [false, true] {
        let (program, entry) = program(text, "App.LazyDerived", "answer", release);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("(7, \"x\")|188417|true"));
        heap.drop_value(value).expect("result release");
        let memory = heap
            .finish()
            .expect("representation field dictionaries released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn every_successful_recursive_prefix_releases_evidence_captures_and_partial_strings() {
    for release in [false, true] {
        let (program, entry) = program(RECURSIVE, "App.Derived", "answer", release);
        let mut completed = false;
        for fuel in 0..=8192 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("outside".into()))
                .expect("caller root");
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], fuel);
            let correct = match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    let correct =
                        heap.string(&value) == Ok("Node (Node Leaf 7 Leaf) 8 Leaf|true|GT|30008");
                    heap.drop_value(value).expect("result release");
                    correct
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => true,
                ProgramOutcome::Raised(value) => {
                    heap.drop_value(value).expect("unexpected failure release");
                    false
                }
                ProgramOutcome::Failed(cause) => {
                    heap.drop_cause(cause).expect("unexpected cause release");
                    false
                }
                ProgramOutcome::Fault(_) => false,
            };
            assert_eq!(heap.string(&external), Ok("outside"));
            assert_eq!(
                heap.stats().live_objects,
                1,
                "release={release} fuel={fuel}"
            );
            heap.drop_value(external).expect("caller root release");
            let memory = heap
                .finish()
                .expect("every actual root released without a sweep");
            assert_eq!(memory.allocations, memory.frees);
            assert!(correct, "release={release} fuel={fuel}");
            if completed {
                break;
            }
        }
        assert!(
            completed,
            "sweep reaches the first successful recursive execution"
        );
    }
}
