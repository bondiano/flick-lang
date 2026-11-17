//! Checked source defaults keep real value/function arity and actual self.
use crate::causes;
use std::fs;
use std::path::Path;

use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{self, Name};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
use flick_model::run;

fn program(text: &str, release: bool, entry: &Name) -> memory::Program {
    let modules = run::compile_source(text).expect("source checks");
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
    perceus::lower(ir.iter().chain(build::builtin_modules()), entry)
        .expect("owned dictionary source lowering")
}

#[test]
fn defaults_and_existing_superclass_dispatch_match_independent_corpus_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/codegen");
    for case in ["defaults", "repeated_superclass"] {
        let file = root.join(format!("{case}.fl"));
        let modules = run::compile(&file).expect("checked corpus source");
        let main = run::main_of(&modules).expect("main export");
        let expected = fs::read_to_string(file.with_extension("stdout")).expect("specified stdout");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
                .unwrap_or_else(|error| panic!("{case} release={release}: {error}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0);
            assert_eq!(ran.memory.allocations, ran.memory.frees);
        }
    }
}

const DEFAULTS: &str = r#"module App.Defaults exposing (answer)
class Render a =
  text : a -> String
  repeated : a -> String
  let repeated value = text value ++ text value
  tagged : Show b => (a, b) -> String
  let tagged (value, tag) = repeated value ++ ":" ++ show tag

instance Render String =
  let text value = value

type Box a = Box a
instance Render a => Render (Box a) =
  let text (Box value) = text value

class Default a =
  make : Unit -> a
  default : a
  let default = make ()

instance Default String =
  let make () = "value"

choose : Default a => Unit -> a
let choose () = default

answer : Unit -> String
let answer () =
  let render (Box { part = (left, right); discarded = _ }) = left ++ right
  let piece = render (Box { part = ("kept", "!"); discarded = "discarded" })
  tagged (Box piece, "tag") ++ "|" ++ (default : String) ++ "|" ++ (choose () : String)
"#;

#[test]
fn contextual_and_value_defaults_keep_dictionary_and_ordinary_arity() {
    let entry = Name::new("App.Defaults", "answer");
    for release in [false, true] {
        let program = program(DEFAULTS, release, &entry);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
        assert_eq!(report.stdout, "");
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("kept!kept!:\"tag\"|value|value"));
        heap.drop_value(value)
            .expect("caller releases returned string");
        let memory = heap.finish().expect("no dictionary cycles or live storage");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn default_signature_evidence_is_passed_after_actual_self() {
    let text = r#"module App.Extra exposing (answer)
class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe value = "<" ++ show value ++ ">"
  tagged : String -> a -> String
  let tagged tag value = tag ++ ":" ++ describe value
instance Describe String
instance Describe Float
answer : Unit -> String
let answer () = describe "kept" ++ "|" ++ tagged "number" (2.5 : Float)
"#;
    let entry = Name::new("App.Extra", "answer");
    for release in [false, true] {
        let program = program(text, release, &entry);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("<\"kept\">|number:<2.5>"));
        heap.drop_value(value).expect("result release");
        let memory = heap.finish().expect("extra evidence released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn explicit_dictionary_in_an_opened_module_keeps_its_lexical_binding() {
    let text = r#"module App.Opened exposing (answer)
class Render a =
  render : a -> String
type module Policy =
  type Key
  key : Key
  renderer : Render.Dict Key
  instance Render Key from renderer
module Argument exposing (Key, renderer, key) =
  type alias Key = String
  let key = "x"
  renderer : Render.Dict Key
  let renderer = Render.dictionary { render = fun _ -> "chosen" }
packed : Module Policy
let packed = pack Argument as Policy
answer : Unit -> String
let answer () =
  unpack packed as P in render P.key
"#;
    let entry = Name::new("App.Opened", "answer");
    for release in [false, true] {
        let program = program(text, release, &entry);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("chosen"));
        heap.drop_value(value).expect("result release");
        let memory = heap.finish().expect("module and dictionary roots released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn defaults_and_constructor_parameters_release_every_interrupted_prefix() {
    let entry = Name::new("App.Defaults", "answer");
    for release in [false, true] {
        let program = program(DEFAULTS, release, &entry);
        let mut completed = false;
        for fuel in 0..=1024 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("outside".into()))
                .expect("caller root");
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], fuel);
            let correct = match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    let correct = heap.string(&value) == Ok("kept!kept!:\"tag\"|value|value");
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
            let memory = heap.finish().expect("every actual execution root released");
            assert_eq!(memory.allocations, memory.frees);
            assert!(correct, "release={release} fuel={fuel}");
        }
        assert!(completed, "fuel sweep must include successful execution");
    }
}
