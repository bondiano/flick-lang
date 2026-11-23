//! Counted-memory capture reclamation, reset, and zero-leak sweeps for O3 local lazy cells.

use flick_core::ir::memory::{Block, Exit, Instruction, Intrinsic, Name, Operation, Program};
use flick_model::memory::{Error, Heap, ProgramOutcome, Reason, Value, execute_program_outcome};

use crate::causes;

use super::{assert_all_fuel_prefixes, compile_counted};

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

const LOCAL_LAZY_MEMORY_SRC: &str = r#"module App.LocalLazyMemory exposing (reclaim_capture, outcomes, cycle, try_compound_reset, catch_defect_compound_reset, ensuring_reset)

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

try_compound_reset : Unit ->{Async} String
let try_compound_reset () =
  let runs = Ref.make 0
  let l = lazy {
    let n = Ref.get runs + 1
    Ref.set runs n
    Host.raw_stdout_line ("try:" ++ show n)
    if n == 1 then
      Effect.fail_cause_sync (fun () ->
        Cause.combine (Cause.interrupt None) (Cause.fail (Boom "compound")))
    else
      "ok"
  }
  let a =
    try await l with
    | Boom s -> s
  let b =
    try await l with
    | Boom s -> s
  let c =
    try await l with
    | Boom s -> s
  a ++ "," ++ b ++ "," ++ c

catch_defect_compound_reset : Unit ->{Async} String
let catch_defect_compound_reset () =
  let runs = Ref.make 0
  let l = lazy_make_local (task {
    let n = Ref.get runs + 1
    Ref.set runs n
    Host.raw_stdout_line ("cd:" ++ show n)
    if n == 1 then
      Effect.fail_cause_sync (fun () ->
        Cause.combine (Cause.interrupt None) (Cause.die "compound-die"))
    else
      "ok"
  })
  let a = Effect.catch_defect (fun msg -> msg) (task { await l })
  let b = await l
  let c = await l
  a ++ "," ++ b ++ "," ++ c

ensuring_reset : Unit ->{Async} String
let ensuring_reset () =
  let runs = Ref.make 0
  let l = lazy_make_local (task {
    let n = Ref.get runs + 1
    Ref.set runs n
    Host.raw_stdout_line ("ens:" ++ show n)
    if n == 1 then
      Effect.fail_cause_sync (fun () -> Cause.interrupt None)
    else
      "from-finalizer"
  })
  Effect.ensuring
    (task {
      let v = await l
      Host.raw_stdout_line ("fin:" ++ v)
    })
    (task { await l })
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

#[test]
fn local_lazy_resets_under_try_compound_interrupt_fail() {
    let entry = Name::new("App.LocalLazyMemory", "try_compound_reset");
    let release_program = compile_counted(LOCAL_LAZY_MEMORY_SRC, true, &entry);
    assert!(
        program_has_intrinsic(&release_program, Intrinsic::LazyMakeLocal),
        "release build must rewrite non-escaping lazy under try to Intrinsic::LazyMakeLocal"
    );
    assert!(
        !program_has_intrinsic(&release_program, Intrinsic::LazyMake),
        "non-escaping lazy under try should not stay Intrinsic::LazyMake in release"
    );
    assert_all_fuel_prefixes(
        LOCAL_LAZY_MEMORY_SRC,
        &entry,
        4_096,
        "try:1\ntry:2\n",
        |heap, val| assert_eq!(heap.string(val), Ok("compound,ok,ok")),
    );
}

#[test]
fn local_lazy_resets_under_catch_defect_compound_interrupt_die() {
    assert_all_fuel_prefixes(
        LOCAL_LAZY_MEMORY_SRC,
        &Name::new("App.LocalLazyMemory", "catch_defect_compound_reset"),
        4_096,
        "cd:1\ncd:2\n",
        |heap, val| assert_eq!(heap.string(val), Ok("compound-die,ok,ok")),
    );
}

#[test]
fn local_lazy_resets_for_ensuring_finalizer() {
    let entry = Name::new("App.LocalLazyMemory", "ensuring_reset");
    for release in [false, true] {
        let program = compile_counted(LOCAL_LAZY_MEMORY_SRC, release, &entry);
        let mut completed = false;
        for fuel in 0..=4_096 {
            let mut heap = Heap::new();
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], fuel);
            match report.outcome {
                ProgramOutcome::Failed(cause) => {
                    assert_eq!(
                        report.stdout, "ens:1\nens:2\nfin:from-finalizer\n",
                        "release={release} fuel={fuel}"
                    );
                    let has_interrupt = heap
                        .cause_reasons(&cause)
                        .expect("cause reasons")
                        .any(|r| matches!(r, Reason::Interrupt { .. }));
                    assert!(
                        has_interrupt,
                        "release={release}: expected interrupt reason"
                    );
                    heap.drop_cause(cause).expect("drop interrupt cause");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => {
                    causes::unexpected(&mut heap, other, "expected interrupt failure or OutOfFuel")
                }
            }
            let stats = heap.finish().expect("zero leaks on ensuring_reset");
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                break;
            }
        }
        assert!(completed, "ensuring_reset release={release} did not finish");
    }
}
