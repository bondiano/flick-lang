//! Stage 4E counted-execution conformance tests: `Channel`, `ChannelSchema`, `Sink`,
//! `Stream`, `SubscriptionRef.changes`, `TxSubscriptionRef.changes_stream`,
//! `runtime/interrupt_skips`, and streaming fuel-prefix leak sweep.

use std::fs;
use std::path::Path;

use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Reason, Value};

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
                "{label} (release={release}): stdout mismatch\nexpected:\n{expected}\ngot:\n{stdout}"
            ));
        }
    }
    Ok(())
}

fn run_suite(fixtures: &[&str], fuel: u64) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    let mut failures = Vec::new();
    for rel in fixtures {
        let path = root.join(rel);
        if let Err(err) = check_fixture(&path, rel, fuel) {
            failures.push(err);
        }
    }
    assert!(
        failures.is_empty(),
        "{} Stage 4E fixture(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

#[test]
fn stage4e_channel_fixtures_match_stdout_with_zero_leaks() {
    run_suite(
        &[
            "tests/run/stdlib/effect/channel.fl",
            "tests/run/stdlib/effect/channel_core.fl",
            "tests/run/stdlib/effect/channel_control.fl",
            "tests/run/stdlib/effect/channel_transform.fl",
            "tests/run/stdlib/effect/channel_schema.fl",
        ],
        20_000_000,
    );
}

#[test]
fn stage4e_sink_fixtures_match_stdout_with_zero_leaks() {
    run_suite(
        &[
            "tests/run/stdlib/effect/sink.fl",
            "tests/run/stdlib/effect/sink_more.fl",
        ],
        20_000_000,
    );
}

#[test]
fn stage4e_stream_fixtures_match_stdout_with_zero_leaks() {
    run_suite(
        &[
            "tests/run/stdlib/effect/stream.fl",
            "tests/run/stdlib/effect/stream_core.fl",
            "tests/run/stdlib/effect/stream_transform.fl",
            "tests/run/stdlib/effect/stream_combine.fl",
            "tests/run/stdlib/effect/stream_errors.fl",
            "tests/run/stdlib/effect/stream_sink.fl",
        ],
        40_000_000,
    );
}

#[test]
fn stage4e_interrupt_skips_matches_stdout_with_zero_leaks() {
    run_suite(&["tests/run/runtime/interrupt_skips.fl"], 20_000_000);
}

#[test]
fn stage5_corpus_fixes_match_stdout_with_zero_leaks() {
    run_suite(
        &[
            "tests/run/stdlib/effect/exit_cause.fl",
            "tests/run/runtime/lazy_abort_and_defect.fl",
            "tests/run/runtime/lazy_shared.fl",
            "tests/run/codegen/service_child.fl",
        ],
        20_000_000,
    );
}

#[test]
fn stage5_lazy_shared_and_service_child_fuel_sweep_has_zero_leaks() {
    let source = r#"
module App.Main exposing (main)
import Deferred
import Effect

service Greeter =
  greet : String ->{} String

type error Nope = Nope String

main : Unit ->{Host.Console, Async; AppError} Unit
let main () =
  let g = { greet = fun who -> "hi " ++ who }
  let (x, y) = provide Greeter g {
    parallel (task { Greeter.greet "a" }) (task { Greeter.greet "b" })
  }
  let gate : Deferred.Deferred none Unit = Deferred.make ()
  let shared = lazy {
    Deferred.await_ gate
    42
  }
  let r2 = scope s {
    let f1 = Fiber.spawn s (fun _ -> ()) (task { await shared })
    let f2 = Fiber.spawn s (fun _ -> ()) (task { await shared })
    Fiber.interrupt f1
    ignore (Deferred.succeed () gate)
    Fiber.join f2
  }
  let runs = Ref.make 0
  let failing = lazy {
    Ref.set runs (Ref.get runs + 1)
    (Fail.fail (Nope "nope") : String)
  }
  let e1 = try await failing with | Nope m -> m
  let e2 = try await failing with | Nope m -> m
  let shared_cnt = Ref.make 0
  let bump = task { for _ <- List.range 0 20 do Ref.modify shared_cnt (fun n -> ((), n + 1)) }
  let _ = parallel_all 4 (List.repeat 10 bump)
  Platform.stdout_line (show (x, y, r2, e1, e2, Ref.get runs, Ref.get shared_cnt))
"#;
    let modules = run::compile_source(source).expect("compile stage5 sweep");
    let main = run::main_of(&modules).expect("main");
    let counted = run::counted_main(&modules, &main, false).expect("lower");
    let mut full_heap = Heap::new();
    let baseline = counted.execute(&mut full_heap, 5_000_000);
    assert!(matches!(
        baseline.outcome,
        ProgramOutcome::Returned(Value::Unit)
    ));
    assert_eq!(
        baseline.stdout,
        "(\"hi a\", \"hi b\", 42, \"nope\", \"nope\", 1, 200)\n"
    );
    baseline.outcome.release(&mut full_heap).expect("release");
    assert_eq!(full_heap.finish().expect("finish").live_objects, 0);

    let mut reached = false;
    let mut fuel = 0u64;
    while fuel <= 1_000_000u64 {
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let exhausted = matches!(report.outcome, ProgramOutcome::Fault(Error::OutOfFuel));
        let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        report.outcome.release(&mut heap).expect("release");
        let stats = heap.finish().expect("finish");
        assert_eq!(stats.live_objects, 0, "fuel={fuel} leaked: {stats:?}");
        assert_eq!(stats.allocations, stats.frees, "fuel={fuel} mismatch");
        if !exhausted {
            assert!(ok, "fuel={fuel} unexpected outcome");
            reached = true;
            break;
        }
        fuel += if fuel < 2_000 {
            7
        } else if fuel < 20_000 {
            47
        } else {
            251
        };
    }
    assert!(reached, "stage5 sweep did not reach terminal fuel");
}

#[test]
fn stage4e_stream_platform_non_host_with_zero_leaks() {
    let source = r#"
module App.Main exposing (main)

main : Unit ->{Host.Console, Async; AppError} Unit
let main () =
  let encoded = Stream.mk_bytes (Stream.encode_text (Stream.make ["hi", " ", "flick"]))
  Platform.stdout_line ("encode: " ++ show (List.length encoded))
  let decoded = Stream.mk_string (Stream.decode_text (Stream.make [[0xEF, 0xBB], [0xBF, 0xCE], [0xB1, 0x21]]))
  Platform.stdout_line ("decode: " ++ decoded)
  let lines = Stream.run_collect (Stream.split_lines (Stream.make ["a\rb", "\nc\r\nd"]))
  Platform.stdout_line ("lines: " ++ show lines)
  let limited = Stream.mk_bytes (Stream.limit_bytes (Option.with_default ByteSize.zero (ByteSize.bytes 3)) (Stream.succeed [99]) (Stream.make [[1, 2], [3, 4]]))
  Platform.stdout_line ("limit: " ++ show limited)
  let sub_out = scope {
    let hub = PubSub.unbounded { replay = Some 2 }
    let sub = PubSub.subscribe hub
    let _ = PubSub.publish 10 hub
    let _ = PubSub.publish 20 hub
    Stream.run_collect (Stream.take 2 (Stream.from_subscription sub))
  }
  Platform.stdout_line ("from_sub: " ++ show sub_out)
"#;
    let modules = run::compile_source(source).expect("compile stream_platform non-host");
    let main = run::main_of(&modules).expect("main");
    let expected = "encode: 8\ndecode: α!\nlines: [\"a\\rb\", \"c\", \"d\"]\nlimit: [1, 2, 99]\nfrom_sub: [10, 20]\n";
    for release in [false, true] {
        let counted = run::counted_main(&modules, &main, release).expect("lower");
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, 20_000_000);
        let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let stdout = report.stdout.clone();
        report.outcome.release(&mut heap).expect("release");
        let stats = heap.finish().expect("finish");
        assert_eq!(stats.live_objects, 0, "leak (release={release}): {stats:?}");
        assert!(
            ok,
            "unexpected outcome (release={release}): stdout={stdout}"
        );
        assert_eq!(stdout, expected, "stdout mismatch (release={release})");
    }
}

#[test]
fn stage4e_streaming_fuel_prefix_sweep_has_zero_leaks() {
    let source = r#"
module App.Main exposing (main)

main : Unit ->{Host.Console, Async; AppError} Unit
let main () =
  let log = Ref.make []
  let res_ch =
    Channel.acquire_use_release
      (task { 10 })
      (fun n -> Channel.from_list [n, n + 1])
      (fun _ _ -> Ref.set log (Ref.get log ++ ["rel"]))
  let xs = Channel.run_collect res_ch
  let par =
    Stream.run_collect
      (Stream.flat_map_par
        { concurrency = Some 2; bufferSize = Some 2 }
        (fun n -> Stream.make [n, n * 2])
        (Stream.make [1, 2]))
  let buf =
    Stream.run_collect
      (Stream.buffer { capacity = Some 2; strategy = None } (Stream.make [3, 4, 5]))
  let merged = Stream.run_count (Stream.merge (Stream.make [1, 2]) (Stream.make [3, 4]))
  let trans = Stream.run_collect (Stream.transduce (Sink.take 2) (Stream.make [1, 2, 3]))
  let peeled = scope {
    let (h, r) = Stream.peel (Sink.take 1) (Stream.make [7, 8])
    h ++ Stream.run_collect r
  }
  Platform.stdout_line (show (List.length xs, List.length par, List.length buf, merged, List.length trans, List.length peeled, Ref.get log))
"#;
    let modules = run::compile_source(source).expect("compile sweep");
    let main = run::main_of(&modules).expect("main");
    let counted = run::counted_main(&modules, &main, false).expect("lower");

    let mut full_heap = Heap::new();
    let baseline = counted.execute(&mut full_heap, 20_000_000);
    assert!(matches!(
        baseline.outcome,
        ProgramOutcome::Returned(Value::Unit)
    ));
    assert_eq!(baseline.stdout, "(2, 4, 3, 4, 2, 2, [\"rel\"])\n");
    baseline.outcome.release(&mut full_heap).expect("release");
    assert_eq!(full_heap.finish().expect("finish").live_objects, 0);

    let mut terminal = None;
    let mut fuel = 0u64;
    while fuel <= 1_000_000u64 {
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let exhausted = matches!(report.outcome, ProgramOutcome::Fault(Error::OutOfFuel));
        let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{:?}", report.outcome);
        report.outcome.release(&mut heap).expect("release");
        let stats = heap.finish().expect("finish");
        assert_eq!(stats.live_objects, 0, "fuel={fuel} leaked: {stats:?}");
        assert_eq!(
            stats.allocations, stats.frees,
            "fuel={fuel} alloc/free: {stats:?}"
        );
        if !exhausted {
            assert!(ok, "fuel={fuel} unexpected outcome: {detail}");
            terminal = Some(fuel);
            break;
        }
        fuel += if fuel < 2_000 {
            7
        } else if fuel < 20_000 {
            47
        } else {
            251
        };
    }
    assert!(
        terminal.is_some(),
        "streaming sweep did not reach terminal fuel"
    );
}
