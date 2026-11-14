//! Structural classes must dispatch through field dictionaries, not raw payloads.
#[path = "support/causes.rs"]
pub mod causes;
use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{self, Name};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
use flick_model::run;

const SOURCE: &str = r#"module App.Structural exposing (answer, super_equal, short_equal, short_compare)
type Token = Token Bool String

instance Eq Token =
  let eq (Token left _) (Token right _) = left == right

instance Ord Token =
  let compare (Token left _) (Token right _) =
    if left == right then EQ else if left then GT else LT

instance Hash Token =
  let hash (Token flag _) = if flag then 17 else 9

instance Show Token =
  let show (Token flag _) = if flag then "T" else "F"

via_ord : Ord a => a -> a -> Bool
let via_ord left right = eq left right

via_hash : Hash a => a -> a -> Bool
let via_hash left right = eq left right

super_equal : Unit -> Bool
let super_equal () =
  let left = { value = Token true "left" }
  let right = { value = Token true "right" }
  via_ord left right && via_hash left right

type Trap = Trap

spin_bool : Unit -> Bool
let rec spin_bool () = spin_bool ()

spin_order : Unit -> Ordering
let rec spin_order () = spin_order ()

instance Eq Trap =
  let eq _ _ = spin_bool ()

instance Ord Trap =
  let compare _ _ = spin_order ()

short_equal : Unit -> Bool
let short_equal () =
  not (eq (Token true "left", Trap) (Token false "right", Trap))

short_compare : Unit -> Bool
let short_compare () =
  match compare (Token true "left", Trap) (Token false "right", Trap) with
  | GT -> true
  | _ -> false

answer : Unit -> String
let answer () =
  let left = { z = Token false "left"; a = Token true "left" }
  let right = { a = Token true "right"; z = Token false "right" }
  let changed = { a = Token false "right"; z = Token true "right" }
  let ordering =
    match compare left changed with
    | LT -> "LT"
    | EQ -> "EQ"
    | GT -> "GT"
  let pair = (Token true "first", Token false "second")
  show (eq left right) ++ "/" ++ show (left == right) ++ "|" ++ ordering ++ "|" ++ show (hash left)
    ++ "|" ++ show left ++ "|" ++ show pair ++ "|" ++ show {} ++ "|" ++ show ()
"#;
// `eq` dispatches through field evidence. The concrete `==` operator instead
// compares ordinary structural payloads, including the ignored String field.
const EXPECTED: &str = "true/false|GT|11107|{ a = T, z = F }|(T, F)|{  }|()";

fn program(release: bool) -> (memory::Program, Name) {
    named_program(release, "answer")
}

fn named_program(release: bool, name: &str) -> (memory::Program, Name) {
    let modules = run::compile_source(SOURCE).expect("structural class source checks");
    let entry = Name::new("App.Structural", name);
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(&modules, module, false)
            } else {
                module.ir.clone()
            }
        })
        .collect();
    let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
        .expect("counted structural dictionaries");
    (program, entry)
}

#[test]
fn superclass_getters_and_short_circuiting_keep_field_dictionary_semantics() {
    for name in ["super_equal", "short_equal", "short_compare"] {
        for release in [false, true] {
            let (program, entry) = named_program(release, name);
            let mut heap = Heap::new();
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
            let ProgramOutcome::Returned(value) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
            };
            assert_eq!(value, Value::Bool(true), "{name} release={release}");
            heap.drop_value(value).expect("result release");
            let memory = heap
                .finish()
                .expect("getter and skipped callback roots released");
            assert_eq!(memory.allocations, memory.frees);
        }
    }
}

#[test]
fn all_four_structural_classes_use_field_methods_and_record_label_order() {
    for release in [false, true] {
        let (program, entry) = program(release);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
        assert_eq!(report.stdout, "");
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok(EXPECTED));
        heap.drop_value(value).expect("caller releases result");
        let memory = heap
            .finish()
            .expect("no structural dictionary storage remains");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn every_structural_method_fuel_prefix_releases_partial_views_and_callbacks() {
    for release in [false, true] {
        let (program, entry) = program(release);
        let mut completed = false;
        for fuel in 0..=2048 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("outside".into()))
                .expect("caller root");
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], fuel);
            let correct = match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    let correct = heap.string(&value) == Ok(EXPECTED);
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
            let memory = heap.finish().expect("all actual internal roots released");
            assert_eq!(memory.allocations, memory.frees);
            assert!(correct, "release={release} fuel={fuel}");
        }
        assert!(completed, "fuel sweep includes completion");
    }
}
