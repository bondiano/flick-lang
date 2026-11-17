//! Checked source error semantics on the explicitly counted path.
//! Expectations follow source values/control flow, not oracle agreement.

use crate::causes;
use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{self, Name};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program};
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
        .expect("counted source lowering")
}

#[test]
fn checked_try_handles_result_failure_and_result_question_in_both_profiles() {
    let text = r#"module App.CountedTry exposing (answer)

type error Missing = Missing Int

step : Int ->{; Missing} String
let step value = if value > 0 then "ok" else from_result (Err (Missing value))

pick : Result Missing String ->{; Missing} String
let pick result = result?

answer : Unit ->{} String
let answer () =
  let first =
    try step 0 with
    | Missing value -> "caught " ++ show value
  let second =
    try step 2 with
    | Missing _ -> "unexpected"
  let third =
    try pick (Err (Missing 7)) with
    | Missing value -> "caught " ++ show value
  let fourth =
    try pick (Ok "kept") with
    | Missing _ -> "unexpected"
  first ++ " | " ++ second ++ " | " ++ third ++ " | " ++ fourth
"#;
    let entry = Name::new("App.CountedTry", "answer");
    for release in [false, true] {
        let program = program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("typed failure is caught, and normal return skips its handler");
        assert_eq!(
            heap.string(&ran.value),
            Ok("caught 0 | ok | caught 7 | kept")
        );
        assert_eq!(ran.stdout, "");
        heap.drop_value(ran.value).expect("result");
        let memory = heap
            .finish()
            .expect("failure payloads and callbacks released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn source_outcome_transfers_uncaught_payload_and_preserves_preceding_stdout() {
    let text = r#"module App.Uncaught exposing (main)

type error Gone = Gone String

raise_gone : String ->{; Gone} Unit
let raise_gone text = from_result (Err (Gone text))

main : Unit ->{; Gone} Unit
let main () =
  Host.raw_stdout_line "before"
  raise_gone "payload"
  Host.raw_stdout_line "unreachable"
"#;
    let modules = run::compile_source(text).expect("typed failure source checks");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let report = run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release)
            .expect("counted source lowering");
        assert_eq!(report.stdout, "before\n");
        let ProgramOutcome::Raised(payload) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.tag(&payload), Ok("App.Uncaught.Gone.Gone"));
        assert_eq!(
            heap.string(heap.field(&payload, 0).expect("error field")),
            Ok("payload")
        );
        assert_eq!(heap.string(&external), Ok("outside"));
        heap.drop_value(payload).expect("caller releases failure");
        heap.drop_value(external).expect("external root");
        let memory = heap
            .finish()
            .expect("all internal roots released, no sweep");
        assert_eq!(memory.allocations, memory.frees);
        assert_eq!(
            run::run_main_counted(&modules, &main, 100_000, release).err(),
            Some(Error::UnhandledFailure.to_string()),
            "legacy wrapper releases the payload before returning its nonowning error"
        );
    }
}

#[test]
fn source_outcome_fault_preserves_stdout_without_collecting_external_objects() {
    let text = r#"module App.FuelFault exposing (main)

burn : Int ->{} Unit
let rec burn value =
  let next = value + 1
  burn next

main : Unit ->{} Unit
let main () =
  Host.raw_stdout_line "before"
  burn 0
  Host.raw_stdout_line "unreachable"
"#;
    let modules = run::compile_source(text).expect("recursive source checks");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let report = run::run_main_counted_outcome(&mut heap, &modules, &main, 1_000, release)
            .expect("counted source lowering");
        assert_eq!(report.stdout, "before\n");
        {
            let diagnostic = "exact outcome expectation";
            let observed_outcome = report.outcome;
            let correct = matches!(&observed_outcome, ProgramOutcome::Fault(Error::OutOfFuel));
            let detail = format!("{observed_outcome:?}");
            observed_outcome
                .release(&mut heap)
                .expect("every owned outcome released before assertion");
            assert!(correct, "{diagnostic}: {detail}");
        };
        assert_eq!(heap.string(&external), Ok("outside"));
        heap.drop_value(external).expect("external root");
        let memory = heap
            .finish()
            .expect("fault releases every execution-owned root");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn error_guards_and_handlers_run_outside_the_catch_that_selected_them() {
    let text = r#"module App.HandlerBoundary exposing (answer)

type error First = First Int
type error Again = Again String

start : Int ->{; First, Again} String
let start value =
  if value == 0 then from_result (Err (First 1)) else from_result (Err (Again "body"))

gate : Int ->{; Again} Bool
let gate value =
  Host.raw_stdout_line ("guard " ++ show value)
  from_result (Err (Again "guard"))

handler_fails : Unit ->{; Again} String
let handler_fails () =
  try start 0 with
  | First value -> from_result (Err (Again ("handler " ++ show value)))
  | Again text -> "wrong inner " ++ text

guard_fails : Unit ->{; Again} String
let guard_fails () =
  try start 0 with
  | First value when gate value -> "wrong guard"
  | First _ -> "wrong fallback"
  | Again text -> "wrong inner " ++ text

unmatched : Unit ->{; Again} String
let unmatched () =
  try start 1 with
  | First _ -> "wrong match"

answer : Unit ->{} String
let answer () =
  let handled =
    try handler_fails () with
    | Again text -> "outer " ++ text
  let guarded =
    try guard_fails () with
    | Again text -> "outer " ++ text
  let propagated =
    try unmatched () with
    | Again text -> "outer " ++ text
  handled ++ " | " ++ guarded ++ " | " ++ propagated
"#;
    let entry = Name::new("App.HandlerBoundary", "answer");
    for release in [false, true] {
        let program = program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("rethrow and guard failure find only the outer catch");
        assert_eq!(ran.stdout, "guard 1\n");
        assert_eq!(
            heap.string(&ran.value),
            Ok("outer handler 1 | outer guard | outer body")
        );
        heap.drop_value(ran.value).expect("result");
        let memory = heap
            .finish()
            .expect("abandoned handlers and failures released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn returning_a_delayed_task_does_not_capture_the_surrounding_try() {
    let text = r#"module App.DelayedTry exposing (main)

type error Gone = Gone String

make : Unit ->{; Gone} Eff {} {Gone} Unit
let make () =
  Host.raw_stdout_line "make"
  task {
    Host.raw_stdout_line "task"
    from_result (Err (Gone "later"))
  }

main : Unit ->{; Gone} Unit
let main () =
  let delayed =
    try make () with
    | Gone _ -> task { Host.raw_stdout_line "wrong handler" }
  Host.raw_stdout_line "returned"
  await delayed
  Host.raw_stdout_line "unreachable"
"#;
    let modules = run::compile_source(text).expect("delayed failure source checks");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let mut heap = Heap::new();
        let report = run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release)
            .expect("delayed failure lowering");
        assert_eq!(report.stdout, "make\nreturned\ntask\n");
        let ProgramOutcome::Raised(payload) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.tag(&payload), Ok("App.DelayedTry.Gone.Gone"));
        assert_eq!(
            heap.string(heap.field(&payload, 0).expect("error field")),
            Ok("later")
        );
        heap.drop_value(payload).expect("failure result");
        let memory = heap
            .finish()
            .expect("completed try did not retain a hidden handler");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn source_lowering_failure_does_not_start_execution_or_touch_external_roots() {
    let text = r#"module App.UnsupportedHost exposing (main)

external_call : Unit ->{} Unit
foreign external_call from global "unmodeledHostOperation"

main : Unit ->{} Unit
let main () =
  Host.raw_stdout_line "must not execute"
  external_call ()
"#;
    let modules = run::compile_source(text).expect("declared foreign source checks");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let before = heap.stats();
        let result = run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release);
        assert!(matches!(result, Err(error) if error.contains("foreign binding")));
        assert_eq!(
            heap.stats(),
            before,
            "lowering has no language-heap effects"
        );
        assert_eq!(heap.string(&external), Ok("outside"));
        heap.drop_value(external).expect("external root");
        heap.finish()
            .expect("only the caller-owned object was allocated");
    }
}
