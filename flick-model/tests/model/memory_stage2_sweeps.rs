//! Stage 2 fuel-prefix root-integrity sweeps with external aliases.

use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{Block, Exit, Instruction, Intrinsic, Name, Operation, Program};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
use flick_model::run;

use crate::causes;

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

fn block_has_intrinsic(block: &Block, target: Intrinsic) -> bool {
    let here = block.instructions.iter().any(|inst| {
        matches!(
            inst,
            Instruction::Let {
                value: Operation::Intrinsic { kind, .. },
                ..
            } if *kind == target
        )
    });
    here || match &block.exit {
        Exit::Return(_) => false,
        Exit::Branch { then_, else_, .. } => {
            block_has_intrinsic(then_, target) || block_has_intrinsic(else_, target)
        }
    }
}

fn program_has_intrinsic(program: &Program, target: Intrinsic) -> bool {
    program
        .functions
        .values()
        .any(|function| block_has_intrinsic(&function.body, target))
}

const LOCAL_LAZY_MEMORY_SRC: &str = r#"module App.LocalLazyMemory exposing (reclaim_capture, outcomes, cycle)

type Box =
  | Box Int
  | Empty
type error Boom = Boom String

effect Stop =
  abort stop : forall a. String ->{} a

make_box : Int ->{} Box
let make_box n =
  Host.raw_stdout_line "mk"
  Box n

read_box : Box ->{} Int
let read_box b =
  match b with
  | Box n -> n
  | Empty -> 0

reclaim_capture : Unit ->{Async} Int
let reclaim_capture () =
  let c1 = make_box 10
  let c2 = make_box 20
  let c3 = make_box 12
  let l = lazy {
    Host.raw_stdout_line "run"
    let sum = read_box c1 + read_box c2 + read_box c3
    let r1 = make_box sum
    let r2 = make_box 0
    read_box r1 + read_box r2
  }
  await l + await l

outcomes : Unit ->{Async} String
let outcomes () =
  let ok_cell = lazy {
    Host.raw_stdout_line "ok"
    make_box 10
  }
  let ok_sum = read_box (await ok_cell) + read_box (await ok_cell)

  let err_cell = lazy {
    Host.raw_stdout_line "err"
    from_result (Err (Boom "boom"))
  }
  let e1 =
    try
      let _ : Int = await err_cell
      "ok"
    with
    | Boom s -> s
  let e2 =
    try
      let _ : Int = await err_cell
      "ok"
    with
    | Boom s -> s

  let abort_runs = Ref.make 0
  let abort_cell = lazy {
    let n = Ref.get abort_runs + 1
    Ref.set abort_runs n
    Host.raw_stdout_line ("ab:" ++ show n)
    if n == 1 then Stop.stop "stopped" else "recovered"
  }
  let a1 =
    try await abort_cell with
    | Stop.stop why -> why
  let a2 =
    try await abort_cell with
    | Stop.stop why -> why
  let a3 =
    try await abort_cell with
    | Stop.stop why -> why

  let die_runs = Ref.make 0
  let die_cell = lazy_make_local (task {
    let n = Ref.get die_runs + 1
    Ref.set die_runs n
    Host.raw_stdout_line ("die:" ++ show n)
    Effect.die "local-defect"
  })
  let d1 = Effect.catch_defect (fun msg -> msg) (task { await die_cell })
  let d2 = Effect.catch_defect (fun msg -> msg) (task { await die_cell })

  let int_runs = Ref.make 0
  let int_cell = lazy_make_local (task {
    let n = Ref.get int_runs + 1
    Ref.set int_runs n
    Host.raw_stdout_line ("int:" ++ show n)
    if n == 1 then
      Effect.fail_cause_sync (fun () -> Cause.interrupt None)
    else
      "resumed"
  })
  let i1 = Effect.exit (task { await int_cell })
  let i2 = await int_cell
  let i3 = await int_cell

  show ok_sum
    ++ "|" ++ e1 ++ "," ++ e2
    ++ "|" ++ a1 ++ "," ++ a2 ++ "," ++ a3
    ++ "|" ++ d1 ++ "," ++ d2
    ++ "|" ++ show (Exit.has_interrupts i1) ++ "," ++ i2 ++ "," ++ i3

cycle : Unit ->{Async} Int
let cycle () =
  let cell : Ref (Option (Lazy {Async} none Int)) = Ref.make None
  let l = lazy_make_local (task {
    match Ref.get cell with
    | Some x -> await x
    | None -> 0
  })
  Ref.set cell (Some l)
  await l
"#;

#[test]
fn local_lazy_reclaims_captures_on_transition() {
    let reclaim_entry = Name::new("App.LocalLazyMemory", "reclaim_capture");
    let debug_program = compile_counted(LOCAL_LAZY_MEMORY_SRC, false, &reclaim_entry);
    let release_program = compile_counted(LOCAL_LAZY_MEMORY_SRC, true, &reclaim_entry);
    assert!(
        program_has_intrinsic(&release_program, Intrinsic::LazyMakeLocal),
        "release build must lower non-escaping lazy to Intrinsic::LazyMakeLocal"
    );
    assert!(
        !program_has_intrinsic(&release_program, Intrinsic::LazyMake),
        "non-escaping lazy should not lower to synchronized Intrinsic::LazyMake in release"
    );

    let run_once = |program: &Program| {
        let mut heap = Heap::new();
        let report = execute_program_outcome(
            &mut heap,
            program,
            &reclaim_entry,
            vec![Value::Unit],
            10_000,
        );
        let ProgramOutcome::Returned(value) = report.outcome else {
            panic!("expected return, got {:?}", report.outcome);
        };
        assert_eq!(report.stdout, "mk\nmk\nmk\nrun\nmk\nmk\n");
        assert_eq!(value, Value::Int(84));
        heap.drop_value(value).expect("drop result");
        let stats = heap.finish().expect("zero leaks");
        assert_eq!(stats.allocations, stats.frees);
        stats
    };
    let debug_stats = run_once(&debug_program);
    let release_stats = run_once(&release_program);
    assert!(
        release_stats.dups < debug_stats.dups,
        "O3 LazyMakeLocal must avoid duping task on Pending -> Running (release={release_stats:?}, debug={debug_stats:?})"
    );
    assert!(
        release_stats.peak_live_objects + 3 <= debug_stats.peak_live_objects,
        "O3 LazyMakeLocal must reclaim task and its 3 captured Boxes on Pending -> Running before allocating result boxes (release={release_stats:?}, debug={debug_stats:?})"
    );
    assert_all_fuel_prefixes(
        LOCAL_LAZY_MEMORY_SRC,
        &reclaim_entry,
        4_096,
        "mk\nmk\nmk\nrun\nmk\nmk\n",
        |_, val| assert_eq!(*val, Value::Int(84)),
    );
}

#[test]
fn local_lazy_outcomes_and_cycle_have_zero_leaks() {
    assert_all_fuel_prefixes(
        LOCAL_LAZY_MEMORY_SRC,
        &Name::new("App.LocalLazyMemory", "outcomes"),
        8_192,
        "ok\nmk\nerr\nab:1\nab:2\ndie:1\nint:1\nint:2\n",
        |heap, val| {
            assert_eq!(
                heap.string(val),
                Ok(
                    "20|boom,boom|stopped,recovered,recovered|local-defect,local-defect|true,resumed,resumed"
                )
            );
        },
    );

    // Self-force cycle defect on `lazy_make_local`: zero leaks in both profiles.
    for release in [false, true] {
        let entry = Name::new("App.LocalLazyMemory", "cycle");
        let program = compile_counted(LOCAL_LAZY_MEMORY_SRC, release, &entry);
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 10_000);
        causes::assert_defect(&mut heap, report.outcome, &Error::LazyCycle);
        let stats = heap.finish().expect("zero leaks on local LazyCycle");
        assert_eq!(stats.allocations, stats.frees);
    }
}
