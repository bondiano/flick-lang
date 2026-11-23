//! Stage 2 fuel-prefix root-integrity sweeps with external aliases.

use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{Name, Program};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
use flick_model::run;

use crate::causes;

mod local_lazy;

fn compile_counted(text: &str, release: bool, entry: &Name) -> Program {
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
    perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        entry,
    )
    .expect("Stage 2 counted lowering")
}

/// Runs `entry` across every fuel prefix `0..=max_fuel` until completion, asserting
/// zero leaked slots and untouched external alias on every single prefix.
fn assert_all_fuel_prefixes(
    text: &str,
    entry: &Name,
    max_fuel: u64,
    expected_stdout: &str,
    expected_value: impl Fn(&Heap, &Value),
) {
    for release in [false, true] {
        let program = compile_counted(text, release, entry);
        let mut completed = false;
        for fuel in 0..=max_fuel {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("external-root".into()))
                .expect("external root");
            let report =
                execute_program_outcome(&mut heap, &program, entry, vec![Value::Unit], fuel);
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(
                        report.stdout, expected_stdout,
                        "{entry:?} release={release} stdout"
                    );
                    expected_value(&heap, &value);
                    heap.drop_value(value).expect("release return value");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => causes::unexpected(&mut heap, other, "expected return or OutOfFuel"),
            }
            assert_eq!(heap.string(&external), Ok("external-root"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            assert_eq!(heap.stats().live_objects, 1);
            heap.drop_value(external).expect("release external alias");
            let stats = heap.finish().expect("zero occupied slots at every prefix");
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                break;
            }
        }
        assert!(
            completed,
            "{entry:?} release={release} did not complete within {max_fuel} fuel"
        );
    }
}

#[test]
fn top_level_destructure_evaluates_once_and_preserves_aliases_at_every_fuel_prefix() {
    let text = r#"module App.Destructure exposing (answer)

pair : (String, Int)
let pair = ("alpha", 42)

let (first, second) = pair

answer : Unit -> String
let answer () = first ++ ":" ++ show second
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.Destructure", "answer"),
        512,
        "",
        |heap, val| assert_eq!(heap.string(val), Ok("alpha:42")),
    );
}

#[test]
fn local_mutation_labeled_loops_and_scope_finalizers_release_roots_at_every_fuel_prefix() {
    let text = r#"module App.Loops exposing (answer)

answer : Unit ->{} Int
let answer () =
  let mut acc = 0
  'outer: for x <- [1, 2, 3] do {
    scope {
      Scope.add_finalizer (task { Host.raw_stdout_line ("fin:" ++ show x) })
      if x == 2 then continue 'outer
      acc = acc + x * 10
      if x == 3 then break 'outer
    }
  }
  acc
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.Loops", "answer"),
        2_048,
        "fin:1\nfin:2\nfin:3\n",
        |_, val| assert_eq!(*val, Value::Int(40)),
    );
}

#[test]
fn nonlocal_option_question_unwinds_and_preserves_aliases_at_every_fuel_prefix() {
    let text = r#"module App.Question exposing (answer)

pick : Option Int -> Option Int -> Option Int
let pick a b =
  let x = a?
  let y = b?
  Some (x + y)

answer : Unit -> String
let answer () = show (pick (Some 3) (Some 4)) ++ "|" ++ show (pick (Some 3) None)
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.Question", "answer"),
        4_096,
        "",
        |heap, val| assert_eq!(heap.string(val), Ok("Some 7|None")),
    );
}

#[test]
fn lazy_seq_generators_zip_and_early_break_release_roots_at_every_fuel_prefix() {
    let text = r#"module App.SeqTest exposing (answer)

answer : Unit ->{} String
let answer () =
  let evens = Seq.from 0 |> Seq.map (fun n -> n * 2) |> Seq.take 4
  let pairs = Seq.zip evens (Seq.of_list ["a", "b", "c"]) |> Seq.to_list
  let folded = Seq.from 1 |> Seq.take 5 |> Seq.fold (fun a x -> a + x) 0
  show pairs ++ "|" ++ show folded
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.SeqTest", "answer"),
        10_000,
        "",
        |heap, val| {
            assert_eq!(
                heap.string(val),
                Ok("[(0, \"a\"), (2, \"b\"), (4, \"c\")]|15")
            );
        },
    );
}

#[test]
fn control_handler_discontinue_and_resume_release_roots_at_every_fuel_prefix() {
    let text = r#"module App.ControlTest exposing (answer)

effect Emit =
  control emit : Int ->{} Unit

answer : Unit ->{} Int
let answer () =
  let a =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "closed-after-arm" })
        Emit.emit 7
        Host.raw_stdout_line "unreachable"
      }
      0
    with
    | Emit.emit x ->
      Host.raw_stdout_line "arm-first"
      x
  let b =
    try
      Emit.emit 2
      Emit.emit 3
      0
    with
    | Emit.emit x -> x + resume ()
  a + b
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.ControlTest", "answer"),
        4_096,
        "arm-first\nclosed-after-arm\n",
        |_, val| assert_eq!(*val, Value::Int(12)),
    );
}

#[test]
fn lazy_caches_success_and_typed_error_retries_defect_and_rejects_cycle() {
    let text = r#"module App.LazyTest exposing (answer, cycle)

type error Boom = Boom String

run_err : Lazy {} {Boom} Int ->{Async} String
let run_err err_cell =
  try
    let _ = await err_cell
    "ok"
  with
  | Boom s -> s

answer : Unit ->{Async} String
let answer () =
  let ok_cell = lazy {
    Host.raw_stdout_line "ok-run"
    41 + 1
  }
  let a = await ok_cell
  let b = await ok_cell
  let err_cell = lazy {
    Host.raw_stdout_line "err-run"
    from_result (Err (Boom "nope"))
  }
  let e1 = run_err err_cell
  let e2 = run_err err_cell
  show a ++ "," ++ show b ++ "," ++ e1 ++ "," ++ e2

cycle : Unit ->{Async} Int
let cycle () =
  let cell : Ref (Option (Lazy {Async} none Int)) = Ref.make None
  let l = lazy {
    match Ref.get cell with
    | Some x -> await x
    | None -> 0
  }
  Ref.set cell (Some l)
  await l
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.LazyTest", "answer"),
        4_096,
        "ok-run\nerr-run\n",
        |heap, val| assert_eq!(heap.string(val), Ok("42,42,nope,nope")),
    );

    for release in [false, true] {
        let entry = Name::new("App.LazyTest", "cycle");
        let program = compile_counted(text, release, &entry);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 10_000);
        causes::assert_defect(&mut heap, report.outcome, &Error::LazyCycle);
        let stats = heap.finish().expect("zero leaks on LazyCycle");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn resource_acquire_release_bracket_and_exit_predicates_release_roots_at_every_prefix() {
    let text = r#"module App.ResourceTest exposing (answer)

type error Fail = Fail String

bracketed : Unit ->{} String
let bracketed () =
  try
    Resource.bracket
      (task { Host.raw_stdout_line "b-acq"; "bk" })
      (fun _ -> from_result (Err (Fail "boom")))
      (fun h -> Host.raw_stdout_line ("b-rel:" ++ h))
  with
  | Fail msg -> msg

answer : Unit ->{} String
let answer () = scope {
  Scope.defer (fun exit ->
    Host.raw_stdout_line ("defer:" ++ show (Exit.is_success exit) ++ "," ++ show (Exit.is_failure exit) ++ "," ++ show (Exit.has_interrupts exit))
  )
  let r = Resource.acquire_release
    (task { Host.raw_stdout_line "acq"; "handle" })
    (fun h -> Host.raw_stdout_line ("rel:" ++ h))
  let b = bracketed ()
  r ++ ":" ++ b
}
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.ResourceTest", "answer"),
        4_096,
        "acq\nb-acq\nb-rel:bk\nrel:handle\ndefer:true,false,false\n",
        |heap, val| assert_eq!(heap.string(val), Ok("handle:boom")),
    );
}

#[test]
fn record_row_primitives_preserve_aliases_at_every_fuel_prefix() {
    let text = r#"module App.RecordRowTest exposing (answer)

answer : Unit -> String
let answer () =
  let base = { a = 1; b = "two" }
  let extended = Record.extend (Record.label : Record.Label "c") true base
  let got = Record.get (Record.label : Record.Label "b") extended
  let removed = Record.remove (Record.label : Record.Label "a") extended
  let proj : { b : String } = Record.project removed
  let merged = Record.merge_disjoint proj { d = 9 }
  let keys = Record.labels merged
  got ++ "|" ++ show keys ++ "|" ++ show merged.d
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.RecordRowTest", "answer"),
        4_096,
        "",
        |heap, val| assert_eq!(heap.string(val), Ok("two|[\"b\", \"d\"]|9")),
    );
}

#[test]
fn defer_and_errdefer_statements_preserve_lifo_and_snapshot_at_every_fuel_prefix() {
    let text = r#"module App.DeferStmtTest exposing (answer)

type error Boom = Boom String

failing : Unit ->{; Boom} String
let failing () = scope {
  defer { Host.raw_stdout_line "f-defer-1" }
  errdefer { Host.raw_stdout_line "f-errdefer-1" }
  defer { Host.raw_stdout_line "f-defer-2" }
  from_result (Err (Boom "kaboom"))
}

answer : Unit ->{} String
let answer () = scope {
  defer { Host.raw_stdout_line "outer-defer" }
  errdefer { Host.raw_stdout_line "outer-errdefer-skipped" }
  let res =
    try failing () with
    | Boom msg -> msg
  let _inner = scope {
    defer { Host.raw_stdout_line "inner-defer" }
    errdefer { Host.raw_stdout_line "inner-errdefer-skipped" }
    Err "data-only"
  }
  res
}
"#;
    assert_all_fuel_prefixes(
        text,
        &Name::new("App.DeferStmtTest", "answer"),
        8_192,
        "f-defer-2\nf-errdefer-1\nf-defer-1\ninner-defer\nouter-defer\n",
        |heap, val| assert_eq!(heap.string(val), Ok("kaboom")),
    );
}
