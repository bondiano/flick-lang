//! Stage 4C counted-execution conformance tests: STM (`Tx`, `TxRef`,
//! `TxDeferred`, `TxSemaphore`, `TxReentrantLock`, `TxQueue`, `TxPubSub`,
//! `TxSubscriptionRef`, `TxChunk`, `TxHashMap`, `TxHashSet`, `TxPriorityQueue`)
//! plus `stdlib/effect/effect_core` and a retry/rollback fuel-prefix sweep.

use std::fs;
use std::path::Path;

use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value};

fn check_fixture(path: &Path, label: &str, fuel: u64) -> Result<(), String> {
    let expected = fs::read_to_string(path.with_extension("stdout"))
        .map_err(|e| format!("{label}: missing .stdout: {e}"))?;
    let modules = run::compile(path).map_err(|e| format!("{label}: compile error:\n{e}"))?;
    let main = run::main_of(&modules).map_err(|e| format!("{label}: main error: {e}"))?;
    for release in [false, true] {
        let counted = run::counted_main(&modules, &main, release)
            .map_err(|e| format!("{label} (release={release}): lowering failed: {e}"))?;
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let outcome_ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{:?}", report.outcome);
        let stdout = report.stdout.clone();
        report
            .outcome
            .release(&mut heap)
            .map_err(|e| format!("{label} (release={release}): release error: {e}"))?;
        let stats = heap
            .finish()
            .map_err(|e| format!("{label} (release={release}): heap leak: {e}"))?;
        if stats.live_objects != 0 || stats.allocations != stats.frees {
            return Err(format!("{label} (release={release}): leak {stats:?}"));
        }
        if !outcome_ok {
            return Err(format!(
                "{label} (release={release}): unexpected outcome {detail}\nstdout:\n{stdout}"
            ));
        }
        if stdout != expected {
            return Err(format!(
                "{label} (release={release}): stdout mismatch\n  actual:   {stdout:?}\n  expected: {expected:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn stage4c_stm_corpus_and_effect_core_match_stdout_with_zero_leaks() {
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "stdlib/effect/effect_core",
        "stdlib/effect/txref",
        "stdlib/effect/txchunk",
        "stdlib/effect/txdeferred",
        "stdlib/effect/txlock",
        "stdlib/effect/txqueue",
        "stdlib/effect/txpubsub",
        "stdlib/effect/txhashmap",
        "stdlib/effect/txhashset",
    ];
    let mut failures = Vec::new();
    for case in cases {
        if let Err(msg) = check_fixture(&run_root.join(format!("{case}.fl")), case, 10_000_000) {
            failures.push(msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} Stage 4C fixtures failed:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

#[test]
fn stage4c_stm_fuel_prefix_sweeps_release_journal_and_waiter_roots() {
    let src = r"module App.Stage4cSweep exposing (main)

type error Boom = Boom

quiet : Failure e ->{} Unit
let quiet _ = ()

main : Unit ->{Async} Unit
let main () =
  let r = TxRef.make 0
  let _ =
    try
      Tx.atomically (task {
        TxRef.set 99 r
        Fail.fail Boom
      })
    with
    | Boom -> ()
  scope s {
    let waiter = Fiber.spawn s quiet (task {
      Tx.atomically (task {
        let v = TxRef.get r
        if v == 0 then Tx.retry () else v
      })
    })
    Effect.yield_now ()
    Tx.atomically (task { TxRef.set 7 r })
    let _ = Fiber.join waiter
    ()
  }
";
    let modules = run::compile_source(src).expect("sweep source compiles");
    let main = run::main_of(&modules).expect("sweep main");
    let counted = run::counted_main(&modules, &main, false).expect("sweep lowers");

    let mut terminal = None;
    for fuel in 0..=20_000u64 {
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let exhausted = matches!(report.outcome, ProgramOutcome::Fault(Error::OutOfFuel));
        let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{:?}", report.outcome);
        report.outcome.release(&mut heap).expect("outcome releases");
        let stats = heap.finish().expect("no leak at fuel prefix");
        assert_eq!(stats.live_objects, 0, "live objects at fuel={fuel}");
        assert_eq!(stats.allocations, stats.frees, "alloc/free at fuel={fuel}");
        if !exhausted {
            assert!(ok, "unexpected terminal outcome at fuel={fuel}: {detail}");
            terminal = Some(fuel);
            break;
        }
    }
    assert!(terminal.is_some(), "sweep did not reach terminal fuel");
}
