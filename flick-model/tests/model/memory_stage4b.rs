//! Stage 4B counted-execution conformance tests: `Latch`, `Semaphore`,
//! `PartitionedSemaphore`, `SynchronizedRef`, `SubscriptionRef`, `Queue`, `PubSub`,
//! `ScopedRef`, `Resource.{manual,use,refresh}`, `Effect.resource_auto`, `RcRef`,
//! `RcMap`, `Cache`, `ScopedCache`, `Pool`, `LayerMap`, `LayerRef`, plus fuel-prefix sweep.

use std::fs;
use std::path::Path;

use flick_model::memory::{Error, Heap, ProgramOutcome, Value};
use flick_model::run;

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
fn stage4b_sync_queue_pubsub_cache_pool_corpus_match_stdout_with_zero_leaks() {
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "stdlib/effect/deferred",
        "stdlib/effect/mutable_ref",
        "stdlib/effect/ref",
        "stdlib/effect/queue",
        "stdlib/effect/pubsub",
        "stdlib/effect/cache",
        "stdlib/effect/pool_outcomes",
    ];
    let mut failures = Vec::new();
    for case in cases {
        if let Err(msg) = check_fixture(&run_root.join(format!("{case}.fl")), case, 10_000_000) {
            failures.push(msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} Stage 4B fixtures failed:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

#[test]
fn stage4b_fuel_prefix_sweeps_release_manual_scope_and_env_roots() {
    let src = r#"module App.Stage4bSweep exposing (main)

main : Unit ->{Async} Unit
let main () =
  scope s {
    let q : Queue.Queue none String = Queue.bounded 2
    let _ = Queue.offer "a" q
    let _ = Queue.take q
    let res : Resource.Resource s none String =
      Resource.manual (task {
        Resource.acquire_release (task { "v:1" }) (fun _ -> ())
      })
    let _ = Resource.use res (fun v -> v)
    Resource.refresh res
    let pool : Pool.Pool s none String =
      Pool.make {
        acquire = task { Resource.acquire_release (task { "v:p" }) (fun _ -> ()) }
        size = 1
        concurrency = None
        targetUtilization = None
      }
    let _ = Pool.use (fun x -> x) pool
    ()
  }
"#;
    let modules = run::compile_source(src).expect("sweep source compiles");
    let main = run::main_of(&modules).expect("sweep main");
    let counted = run::counted_main(&modules, &main, false).expect("sweep lowers cleanly");

    let mut terminal = None;
    for fuel in 0..=20_000 {
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let done = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let oof = matches!(report.outcome, ProgramOutcome::Fault(Error::OutOfFuel));
        assert!(
            done || oof,
            "fuel={fuel}: unexpected outcome {:?}",
            report.outcome
        );
        report.outcome.release(&mut heap).expect("release outcome");
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("fuel={fuel}: leaked manual-scope/env root: {e}"));
        assert_eq!(stats.live_objects, 0, "fuel={fuel}: {stats:?}");
        assert_eq!(stats.allocations, stats.frees, "fuel={fuel}: {stats:?}");
        if done {
            terminal = Some(fuel);
            break;
        }
    }
    assert!(
        terminal.is_some(),
        "Stage 4B sweep program should complete within 20000 fuel"
    );
}
