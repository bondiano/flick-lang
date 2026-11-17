//! Source expectations for real counted dynamic handler boundaries.
use std::fs;
use std::path::Path;

use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{self, Name};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, execute_program_outcome};
use flick_model::run;

const CAPTURES: &str = r#"module App.HandlerCaptures exposing (answer)

effect Read =
  read : String ->{} String

type error Failed = Failed String

answer : String ->{} String
let answer seed =
  let body_prefix = seed ++ "/body"
  let arm_prefix = seed ++ "/arm"
  let ret_prefix = seed ++ "/ret"
  let error_prefix = seed ++ "/error"
  let good =
    try
      Platform.stdout_line "body"
      Read.read body_prefix
    with
    | Read.read text ->
      Platform.stdout_line "arm"
      arm_prefix ++ ":" ++ text
    | return text ->
      Platform.stdout_line "ret"
      ret_prefix ++ ":" ++ text
    | Failed text -> error_prefix ++ ":" ++ text
  let failed =
    try
      Platform.stdout_line "failing"
      ignore (Read.read body_prefix)
      from_result (Err (Failed body_prefix))
    with
    | Read.read text ->
      Platform.stdout_line "arm"
      arm_prefix ++ ":" ++ text
    | return text ->
      Platform.stdout_line "wrong ret"
      ret_prefix ++ ":" ++ text
    | Failed text ->
      Platform.stdout_line "error"
      error_prefix ++ ":" ++ text
  good ++ "|" ++ failed
"#;

const CAPTURE_RESULT: &str = "seed/ret:seed/arm:seed/body|seed/error:seed/body";
const CAPTURE_STDOUT: &str = "body\narm\nret\nfailing\narm\nerror\n";

const BOUNDARIES: &str = r#"module App.HandlerErrors exposing (answer, uncaught)

effect Ask =
  ask : Unit ->{} String

type error Gone = Gone String

uncaught : String ->{; Gone} String
let uncaught seed =
  try
    Platform.stdout_line "body"
    try Ask.ask () with
    | Gone _ -> "wrong body catch"
  with
  | Ask.ask () ->
    Platform.stdout_line "arm"
    from_result (Err (Gone (seed ++ "/tail")))
  | Gone _ -> "wrong selected catch"
  | return _ -> "wrong return"

answer : String ->{} String
let answer seed =
  let tail =
    try uncaught seed with
    | Gone text -> "outer:" ++ text
  let body =
    try
      ignore (Ask.ask ())
      from_result (Err (Gone (seed ++ "/body")))
    with
    | Ask.ask () -> "ignored operation result"
    | Gone text -> "caught:" ++ text
    | return _ -> "wrong return"
  tail ++ "|" ++ body
"#;

const ABORT: &str = r#"module App.HandlerAbort exposing (answer)

effect Exit =
  abort stop : forall a. String ->{} a

type error Gone = Gone String

answer : String ->{} String
let answer seed =
  try
    try
      try
        Platform.stdout_line "before"
        Exit.stop seed
        Platform.stdout_line "wrong continuation"
        "wrong body value"
      with
      | Gone _ -> "wrong body catch"
    with
    | Exit.stop text -> "abort:" ++ text
    | Gone _ -> "wrong selected catch"
    | return _ -> "wrong return"
  with
  | Gone _ -> "wrong outer catch"
"#;

const DISPATCH: &str = r#"module App.HandlerDispatch exposing (answer)

effect Render =
  render : (Show a, Show b) => a -> b ->{} String
  other : Unit ->{} String

answer : String ->{} String
let answer seed =
  let partial =
    try Render.render 7 with
    | Render.render _ _ -> "wrong old partial provider"
    | Render.other () -> "wrong old other"
  let delayed =
    try task { Render.render true 8 } with
    | Render.render _ _ -> "wrong old task provider"
    | Render.other () -> "wrong old other"
  let closure =
    try (fun () -> Render.render false 9) with
    | Render.render _ _ -> "wrong old closure provider"
    | Render.other () -> "wrong old other"
  try
    try partial false ++ "|" ++ await delayed ++ "|" ++ closure () with
    | Render.other () -> "wrong inner other"
  with
  | Render.render first second -> seed ++ ":" ++ show first ++ "/" ++ show second
  | Render.other () -> "wrong outer other"
"#;

fn programs(text: &str, module: &str, name: &str) -> (Name, [memory::Program; 2]) {
    let modules = run::compile_source(text).expect("source checks");
    let entry = Name::new(module, name);
    let programs = [false, true].map(|release| {
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
        perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("counted source lowering")
    });
    (entry, programs)
}

/// Consumes every successful/failure outcome token, even for a wrong outcome.
fn release_outcome(heap: &mut Heap, outcome: ProgramOutcome, expected: &str, raised: bool) -> bool {
    match outcome {
        ProgramOutcome::Returned(value) => {
            let correct = !raised && heap.string(&value) == Ok(expected);
            heap.drop_value(value)
                .expect("caller releases returned token");
            correct
        }
        ProgramOutcome::Raised(value) => {
            let correct = raised
                && heap.tag(&value) == Ok("App.HandlerErrors.Gone.Gone")
                && heap
                    .field(&value, 0)
                    .is_ok_and(|text| heap.string(text) == Ok(expected));
            heap.drop_value(value)
                .expect("caller releases raised token");
            correct
        }
        ProgramOutcome::Failed(cause) => {
            heap.drop_cause(cause).expect("unexpected cause release");
            false
        }
        ProgramOutcome::Fault(_) => false,
    }
}

fn assert_string_run(text: &str, module: &str, expected: &str, stdout: &str) {
    let (entry, programs) = programs(text, module, "answer");
    for program in programs {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("seed".into()))
            .expect("caller-owned seed");
        let argument = heap.dup(&external).expect("independent caller alias");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], 100_000);
        let detail = format!("{report:?}");
        let correct = release_outcome(&mut heap, report.outcome, expected, false);
        assert_eq!(heap.string(&external), Ok("seed"));
        assert_eq!(heap.stats().live_objects, 1, "only the caller root remains");
        heap.drop_value(external).expect("external root");
        let memory = heap.finish().expect("all actual source heap slots freed");
        assert_eq!(memory.allocations, memory.frees);
        assert!(correct, "{detail}");
        assert_eq!(report.stdout, stdout);
    }
}

#[test]
fn source_handlers_and_forwarded_operations_run_in_both_profiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in [
        "codegen/handlers",
        "codegen/partial_handlers",
        "codegen/prototype_operation",
    ] {
        let modules = run::compile(&root.join(format!("{case}.fl"))).expect("source checks");
        let main = run::main_of(&modules).expect("main export");
        let expected =
            fs::read_to_string(root.join(format!("{case}.stdout"))).expect("specified stdout");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
                .unwrap_or_else(|error| panic!("{case} release={release}: {error}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0);
            assert_eq!(ran.memory.allocations, ran.memory.frees);
        }
    }
}

#[test]
fn captured_body_operation_return_and_error_closures_have_distinct_lifetimes() {
    assert_string_run(
        CAPTURES,
        "App.HandlerCaptures",
        CAPTURE_RESULT,
        CAPTURE_STDOUT,
    );
}

#[test]
fn tail_failures_bypass_body_and_selected_handler_catches_but_body_errors_do_not() {
    assert_string_run(
        BOUNDARIES,
        "App.HandlerErrors",
        "outer:seed/tail|caught:seed/body",
        "body\narm\n",
    );
}

#[test]
fn abort_passes_error_catches_and_skips_return_and_body_continuation() {
    assert_string_run(ABORT, "App.HandlerAbort", "abort:seed", "before\n");
}

#[test]
fn dictionaries_partial_applications_tasks_and_closures_use_the_current_handler() {
    assert_string_run(
        DISPATCH,
        "App.HandlerDispatch",
        "seed:7/false|seed:true/8|seed:false/9",
        "",
    );
}

fn assert_fuel_cuts(
    text: &str,
    module: &str,
    name: &str,
    expected: &str,
    stdout: &str,
    raised: bool,
) {
    let (entry, programs) = programs(text, module, name);
    for program in programs {
        let mut completed = 0;
        let mut exhausted = 0;
        let mut partial_stdout = false;
        for fuel in 0..=2048 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("seed".into()))
                .expect("caller-owned seed");
            let argument = heap.dup(&external).expect("independent caller alias");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
            let prefix = stdout.starts_with(&report.stdout);
            let detail = format!("fuel {fuel}: {report:?}");
            let correct = match report.outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) => {
                    exhausted += 1;
                    partial_stdout |= !report.stdout.is_empty() && report.stdout != stdout;
                    true
                }
                outcome => {
                    completed += 1;
                    let value = release_outcome(&mut heap, outcome, expected, raised);
                    value && report.stdout == stdout
                }
            };
            assert_eq!(heap.string(&external), Ok("seed"), "{detail}");
            assert_eq!(heap.stats().live_objects, 1, "{detail}");
            heap.drop_value(external).expect("external root");
            let memory = heap
                .finish()
                .unwrap_or_else(|error| panic!("{detail}: {error}"));
            assert_eq!(memory.allocations, memory.frees, "{detail}");
            assert!(prefix, "{detail}");
            assert!(correct, "{detail}");
        }
        assert!(completed > 0 && exhausted > 0 && partial_stdout);
    }
}

#[test]
fn every_fuel_cut_releases_returning_handler_captures_without_sweeping_aliases() {
    assert_fuel_cuts(
        CAPTURES,
        "App.HandlerCaptures",
        "answer",
        CAPTURE_RESULT,
        CAPTURE_STDOUT,
        false,
    );
}

#[test]
fn every_fuel_cut_releases_parked_tail_failure_roots_and_owned_raised_outcomes() {
    assert_fuel_cuts(
        BOUNDARIES,
        "App.HandlerErrors",
        "uncaught",
        "seed/tail",
        "body\narm\n",
        true,
    );
}
