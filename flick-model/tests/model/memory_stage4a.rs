//! Stage 4A counted-execution conformance tests: cooperative fibers, C0-C6
//! owner closing, Deferred, FiberSet/Map/Handle, parallel*/race*/timeout*,
//! dynamic Layer/MemoMap/Context, and exhaustive fuel-prefix zero-leak sweeps.

use std::fs;
use std::path::Path;

use flick_core::build;
use flick_core::ir::memory::Name;
use flick_core::opt::perceus;
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_main_program_outcome};

fn check_fixture(path: &Path, label: &str, fuel: u64) -> Result<(), String> {
    let expected = fs::read_to_string(path.with_extension("stdout"))
        .map_err(|e| format!("{label}: missing .stdout: {e}"))?;
    let expects_nonzero = fs::read_to_string(path.with_extension("exit"))
        .ok()
        .is_some_and(|code| code.trim() != "0");
    let modules = run::compile(path).map_err(|e| format!("{label}: compile error:\n{e}"))?;
    let main = run::main_of(&modules).map_err(|e| format!("{label}: main error: {e}"))?;
    for release in [false, true] {
        let counted = run::counted_main(&modules, &main, release)
            .map_err(|e| format!("{label} (release={release}): lowering failed: {e}"))?;
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let outcome_ok = if expects_nonzero {
            matches!(
                report.outcome,
                ProgramOutcome::Raised(_) | ProgramOutcome::Failed(_)
            )
        } else {
            matches!(report.outcome, ProgramOutcome::Returned(Value::Unit))
        };
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

fn run_fixture(path: &Path, label: &str, fuel: u64) {
    if let Err(msg) = check_fixture(path, label, fuel) {
        panic!("{msg}");
    }
}

#[test]
fn stage4a_rt_suites_match_stdout_with_zero_leaks() {
    let rt_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/rt");
    for case in ["layers", "interrupt_closed"] {
        run_fixture(
            &rt_root.join(format!("{case}.fl")),
            &format!("rt/{case}"),
            5_000_000,
        );
    }
}

#[test]
#[ignore = "million-step stress fixtures; run in --release with --ignored"]
fn stage4a_rt_stress_suites_match_stdout_with_zero_leaks() {
    let rt_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/rt");
    for case in ["deep_async", "wake_chain"] {
        run_fixture(
            &rt_root.join(format!("{case}.fl")),
            &format!("rt/{case}"),
            200_000_000,
        );
    }
}

#[test]
fn stage4a_runtime_and_effect_corpus_match_stdout_with_zero_leaks() {
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "runtime/abort_cleanup_die",
        "runtime/abort_closes_scopes",
        "runtime/abort_error_combinators",
        "runtime/abort_stops_children",
        "runtime/arena_finalizers",
        "runtime/arena_layer_finalizers",
        "runtime/arena_layer_scope",
        "runtime/cause_lists",
        "runtime/cleanup_control",
        "runtime/closed_owner",
        "runtime/closing_child_fails",
        "runtime/closing_hooks",
        "runtime/counted_layer_merge",
        "runtime/defer_make",
        "runtime/discontinued_body",
        "runtime/discontinued_fiber",
        "runtime/error_map",
        "runtime/exits",
        "runtime/group_in_closing",
        "runtime/handoff",
        "runtime/interrupt_after_close",
        "runtime/interrupt_in_finalizer",
        "runtime/owned_close",
        "runtime/parallel",
        "runtime/ref_linearizable",
        "runtime/scope_owners",
        "runtime/scope_releases",
        "runtime/shared_layer",
        "runtime/supervisor_defect",
        "stdlib/effect/clock",
        "stdlib/effect/clock_random",
        "stdlib/effect/effect_errors",
        "stdlib/effect/layer_finalizer_services",
        "stdlib/effect/schedule",
        "stdlib/effect/shared_layer_merged",
    ];
    let mut failures = Vec::new();
    for case in cases {
        if let Err(msg) = check_fixture(&run_root.join(format!("{case}.fl")), case, 5_000_000) {
            failures.push(msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} Stage 4A fixtures failed:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

#[test]
fn stage4a_fuel_prefix_sweeps_release_every_async_root() {
    let src = r#"module App.Stage4aSweep exposing (main)

service Tag =
  get : Unit ->{} String

main : Unit ->{Async} Unit
let main () =
  let layer =
    Layer.from_build_memo (fun memo ->
      Scope.add_finalizer (task { () })
      Layer.build_with_memo_map memo (Layer.succeed Tag { get = fun () -> "ok" })
    )
  provide layer {
    let d : Deferred.Deferred none String = Deferred.make ()
    scope owner {
      let set : FiberSet.FiberSet owner none Unit = FiberSet.make ()
      let _ = FiberSet.run { propagateInterruption = false; startImmediately = true } (task { Effect.never () }) set
      let f = Fiber.spawn owner (fun _ -> ()) (task {
        let _ = Deferred.succeed (Tag.get ()) d
        ()
      })
      let (a, b) =
        parallel
          (task { Deferred.await_ d })
          (task { race_first (task { Effect.yield_now (); "r" }) (task { Effect.never () }) })
      let _ = Fiber.join f
      let _ = timeout_option 5 (task { a ++ b })
      ()
    }
  }
"#;
    let modules = run::compile_source(src).expect("sweep source compiles");
    let entry = Name::new("App.Stage4aSweep", "main");
    let ir: Vec<_> = modules
        .iter()
        .map(|m| build::optimized(&modules, m, false))
        .collect();
    let program = perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        &entry,
    )
    .expect("sweep lowers cleanly");

    let mut terminal = None;
    for fuel in 0..=4000 {
        let mut heap = Heap::new();
        let report = execute_main_program_outcome(&mut heap, &program, &entry, fuel);
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
            .unwrap_or_else(|e| panic!("fuel={fuel}: leaked async root: {e}"));
        assert_eq!(stats.live_objects, 0, "fuel={fuel}: {stats:?}");
        assert_eq!(stats.allocations, stats.frees, "fuel={fuel}: {stats:?}");
        if done {
            terminal = Some(fuel);
            break;
        }
    }
    assert!(
        terminal.is_some(),
        "async sweep program should complete within 4000 fuel"
    );
}
