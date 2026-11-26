//! Stage 4D counted-execution conformance tests: `Scheduler`, `ExecutionPlan`,
//! `Events`, `ErrorReporter`, `Cause.pretty`/`annotate`, `LogLevel`, `References`,
//! `Tracer`, `Logger`, `Effect.Observe`, `Metric`, `Request`/`RequestResolver`, `Crypto`,
//! `HMAC`, `JWT`, `Password`, and fuel-prefix leak sweep.

use std::fs;
use std::path::Path;

use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Reason, Value};

mod wave2;

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
        let detail = match &report.outcome {
            ProgramOutcome::Failed(tok) => {
                let mut parts = Vec::new();
                if let Ok(rs) = heap.cause_reasons(tok) {
                    for r in rs {
                        match r {
                            Reason::Die(v) => {
                                let msg = heap
                                    .defect(v)
                                    .map(|d| format!("{d:?}"))
                                    .or_else(|_| heap.string(v).map(str::to_owned))
                                    .unwrap_or_else(|_| format!("{v:?}"));
                                parts.push(format!("Die({msg:?})"));
                            }
                            other => parts.push(format!("{other:?}")),
                        }
                    }
                }
                format!("Failed({parts:?})")
            }
            other => format!("{other:?}"),
        };
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
fn stage4d_wave1_observability_request_crypto_corpus_match_stdout_with_zero_leaks() {
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "stdlib/effect/fibers",
        "stdlib/effect/layer",
        "stdlib/effect/layer_map",
        "stdlib/effect/effect_errors_more",
        "stdlib/effect/tracer",
        "stdlib/effect/observe",
        "stdlib/effect/metric",
        "stdlib/effect/request",
        "stdlib/effect/crypto",
        "stdlib/effect/hmac",
        "stdlib/effect/jwt",
        "stdlib/effect/password",
    ];
    let mut failures = Vec::new();
    for case in cases {
        if let Err(msg) = check_fixture(&run_root.join(format!("{case}.fl")), case, 10_000_000) {
            failures.push(msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} Stage 4D Wave 1 fixtures failed:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

#[test]
fn stage4d_fuel_prefix_sweeps_release_observability_metric_and_request_roots() {
    let src = r#"module App.Stage4dSweep exposing (main)

opts : Tracer.SpanOptions
let opts = {
  attributes = [("k", "v")]
  links = []
  parent = None
  root = false
  kind_ = Tracer.internal
  sampled = None
  level = None
  captureStackTrace = false
  disablePropagation = false
}

main : Unit ->{Async} Unit
let main () =
  let c = Metric.counter "sw_c" { description = None; attributes = []; incremental = true }
  Tracer.with_span "top" opts (task {
    c |> Metric.update 1
    let r = RequestResolver.from_function (fun e -> Request.request e + 1)
    let _ = parallel (task { Request.run r (Request.of 1) }) (task { Request.run r (Request.of 2) })
    ()
  })
"#;
    let modules = run::compile_source(src).expect("sweep source compiles");
    let main = run::main_of(&modules).expect("sweep main");
    let counted = run::counted_main(&modules, &main, false).expect("sweep lowers");

    let mut terminal = None;
    for fuel in 0..=40_000u64 {
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
