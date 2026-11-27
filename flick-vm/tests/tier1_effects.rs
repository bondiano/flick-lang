//! Effect handlers, one-shot continuations, and second-resume defect span tests
//! for `flick_vm::tier1`.

use std::io;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::Severity;
use flick_core::ir::memory::{MemorySpan, Name, Program};
use flick_core::opt::perceus;
use flick_core::source::{FileId, SourceMap};
use flick_core::{build, project};
use flick_vm::stdlib;
use flick_vm::tier0::{self, DefectCode, Error, Heap, ProgramOutcome, Reason};
use flick_vm::tier1;

struct MemHost<'a>(&'a str);

impl project::Host for MemHost<'_> {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        if p == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn lower_source(src: &str, entry: &Name) -> (Program, SourceMap) {
    let mut sources = SourceMap::default();
    let (diags, comp) = project::compile(
        &MemHost(src),
        &mut sources,
        None,
        &[PathBuf::from("/Main.fl")],
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "compile failed: {diags:?}"
    );
    let modules: Vec<_> = comp
        .modules
        .iter()
        .map(|m| (*m.ir).clone())
        .chain(build::builtin_modules().iter().cloned())
        .collect();
    let decls: Vec<_> = comp
        .modules
        .iter()
        .map(|m| m.declarations.as_ref())
        .chain(build::builtin_declarations().iter().map(AsRef::as_ref))
        .collect();
    let program = perceus::lower_roots_with_declarations(modules.iter(), decls, &[entry])
        .expect("perceus lower");
    (program, sources)
}

fn assert_source_returns_string_matching_tier0(src: &str, expected: &str, fuel: u64) {
    let entry = Name::new("App.Main", "main");
    let (program, _) = lower_source(src, &entry);

    let mut heap0 = Heap::new();
    let r0 = tier0::execute_main_program_outcome(&mut heap0, &program, &entry, fuel);
    let mut heap1 = Heap::new();
    let r1 = tier1::execute_main_program_outcome(&mut heap1, &program, &entry, fuel)
        .expect("tier1 prepare");

    let ProgramOutcome::Returned(v0) = &r0.outcome else {
        panic!("tier0 expected Returned, got {:?}", r0.outcome);
    };
    let ProgramOutcome::Returned(v1) = &r1.outcome else {
        panic!("tier1 expected Returned, got {:?}", r1.outcome);
    };
    let s0 = stdlib::show_value(&heap0, v0).expect("show tier0");
    let s1 = stdlib::show_value(&heap1, v1).expect("show tier1");
    assert_eq!(s1, expected, "tier1 value mismatch");
    assert_eq!(s0, s1, "tier0 vs tier1 value mismatch");

    r0.outcome.release(&mut heap0).expect("release tier0");
    heap0.finish().expect("zero leaks tier0");
    r1.outcome.release(&mut heap1).expect("release tier1");
    heap1.finish().expect("zero leaks tier1");

    for prefix in [1, 5, 17, 45, 120, 350, 900] {
        let mut heap = Heap::new();
        let report =
            tier1::execute_main_program_outcome(&mut heap, &program, &entry, prefix).expect("prep");
        assert!(
            matches!(
                report.outcome,
                ProgramOutcome::Returned(_) | ProgramOutcome::Fault(Error::OutOfFuel)
            ),
            "unexpected prefix outcome at fuel {prefix}: {:?}",
            report.outcome
        );
        report.outcome.release(&mut heap).expect("release prefix");
        heap.finish().expect("zero leaks on prefix");
    }
}

#[test]
fn deep_tail_abort_control_and_partial_handlers_match_tier0() {
    let source = "\
module App.Main exposing (main)

effect Ask =
  ask : Unit ->{} Int

effect Stop =
  abort stop : forall a. String ->{} a

effect Emit =
  control emit : Int ->{} Unit

effect Step =
  control step : Int ->{} Int

effect PairOps =
  left : Unit ->{} Int
  right : Unit ->{} Int

type error Boom = Boom String

deep_count : Int ->{Step} Int
let rec deep_count n =
  if n == 0 then 0 else deep_count (Step.step n)

run_tail : Unit ->{} Int
let run_tail () =
  try
    let x = Ask.ask ()
    let y =
      try Ask.ask () with
      | Ask.ask () -> 1
    x + y + Ask.ask ()
  with
  | Ask.ask () -> 10

run_abort : Unit ->{} String
let run_abort () =
  try
    let _ = Stop.stop \"halted\"
    \"unreachable\"
  with
  | Stop.stop msg -> \"aborted:\" ++ msg

run_control : Unit ->{} (List Int, String)
let run_control () =
  try
    Emit.emit 1
    Emit.emit 2
    Emit.emit 3
    \"done\"
  with
  | Emit.emit x ->
    let (xs, s) = resume ()
    ([x] ++ xs, s)
  | return s -> ([], s)

run_partial : Unit ->{} Int
let run_partial () =
  try
    try PairOps.left () + PairOps.right () with
    | PairOps.left () -> 4
  with
  | PairOps.left () -> 40
  | PairOps.right () -> 50

run_error : Unit ->{} String
let run_error () =
  try
    let _ = Fail.fail (Boom \"oops\")
    \"ok\"
  with
  | Boom s -> \"caught:\" ++ s

main : Unit ->{} String
let main () =
  let f =
    try deep_count 1500 with
    | Step.step n -> resume (n - 1)
  show (run_tail ())
    ++ \"|\"
    ++ run_abort ()
    ++ \"|\"
    ++ show (run_control ())
    ++ \"|\"
    ++ show (run_partial ())
    ++ \"|\"
    ++ run_error ()
    ++ \"|\"
    ++ show f
";
    let expected = "\"21|aborted:halted|([1, 2, 3], \\\"done\\\")|54|caught:oops|0\"";
    assert_source_returns_string_matching_tier0(source, expected, 500_000);
}

#[test]
#[allow(clippy::too_many_lines)]
fn second_resume_reports_spec_defect_with_operation_span_and_zero_leaks() {
    let source = "\
module App.Main exposing (main)

effect Choice =
  control flip : Unit ->{} Int

main : Unit ->{} Int
let main () =
  try
    Choice.flip () + 10
  with
  | Choice.flip () ->
    let first = resume 1
    let second = resume 2
    first + second
";
    let entry = Name::new("App.Main", "main");
    let (program, sources) = lower_source(source, &entry);
    let mut heap = Heap::new();
    let report =
        tier1::execute_main_program_outcome(&mut heap, &program, &entry, 100_000).expect("prep");

    let ProgramOutcome::Failed(cause) = &report.outcome else {
        panic!(
            "expected ProgramOutcome::Failed(ResumeTwice), got {:?}",
            report.outcome
        );
    };
    let reasons: Vec<_> = heap.cause_reasons(cause).expect("reasons").collect();
    assert_eq!(reasons.len(), 1, "expected single Die reason");
    let Reason::Die(val) = reasons[0] else {
        panic!("expected Reason::Die, got {:?}", reasons[0]);
    };
    let spec = heap.defect(val).expect("defect spec");
    assert_eq!(spec.code(), &DefectCode::ResumeTwice);
    let Some(MemorySpan { file, start, end }) = spec.span() else {
        panic!("expected source span on ResumeTwice defect");
    };
    let text = &sources.get(FileId::new(file)).text;
    let snippet = &text[start as usize..end as usize];
    assert!(
        snippet.contains("resume") && snippet.contains('2'),
        "expected span to point at second resume operation, got {snippet:?}"
    );

    report.outcome.release(&mut heap).expect("release");
    heap.finish().expect("zero leaks on ResumeTwice");

    let tail_source = "\
module App.Main exposing (main)

effect Choice =
  control flip : Unit ->{} Int

main : Unit ->{} Int
let main () =
  try
    Choice.flip () + 10
  with
  | Choice.flip () ->
    let _ = resume 1
    resume 99
";
    let (tail_program, tail_sources) = lower_source(tail_source, &entry);
    let mut heap = Heap::new();
    let tail_report =
        tier1::execute_main_program_outcome(&mut heap, &tail_program, &entry, 100_000)
            .expect("prep");
    let ProgramOutcome::Failed(cause) = &tail_report.outcome else {
        panic!(
            "expected Failed(ResumeTwice), got {:?}",
            tail_report.outcome
        );
    };
    let reasons: Vec<_> = heap.cause_reasons(cause).expect("reasons").collect();
    let Reason::Die(val) = reasons[0] else {
        panic!("expected Die");
    };
    let spec = heap.defect(val).expect("defect spec");
    assert_eq!(spec.code(), &DefectCode::ResumeTwice);
    let Some(MemorySpan { file, start, end }) = spec.span() else {
        panic!("expected source span on tail ResumeTwice defect");
    };
    let snippet = &tail_sources.get(FileId::new(file)).text[start as usize..end as usize];
    assert!(
        snippet.contains("resume") && snippet.contains("99"),
        "expected span to point at tail second resume, got {snippet:?}"
    );
    tail_report.outcome.release(&mut heap).expect("release");
    heap.finish().expect("zero leaks on tail ResumeTwice");
}
