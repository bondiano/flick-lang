//! Wave 5.2C regression tests for the explicit counted Host/FFI bridge:
//! all 6 `tests/run/ffi/*` fixtures, all 3 `tests/rt/owner_*` fixtures,
//! `examples/first-slice`, and fuel-prefix zero-leak sweeps asserting zero
//! leaked Rust heap slots AND zero leaked Node host handles.

use std::fs;
use std::path::{Path, PathBuf};

use flick_model::run::{compile, compile_files, counted_main_with_bridge, main_of};
use flick_vm::tier0::{
    Error as MemoryError, Heap, HostBridge, HostMode, ProgramOutcome, Reason, Value,
};

const FUEL: u64 = 10_000_000;

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join(rel)
}

fn run_bridged_fixture(bridge: &mut HostBridge, rel: &str) {
    let path = repo_path(rel);
    let expected_stdout = fs::read_to_string(path.with_extension("stdout"))
        .unwrap_or_else(|e| panic!("read stdout for {rel}: {e}"));
    let expect_fail = path.with_extension("exit").exists();
    let stderr_contains = fs::read_to_string(path.with_extension("stderr.contains")).ok();

    let modules = compile(&path).unwrap_or_else(|e| panic!("compile {rel}: {e}"));
    let main = main_of(&modules).unwrap_or_else(|e| panic!("main of {rel}: {e}"));

    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main_with_bridge(&modules, &main, release)
            .unwrap_or_else(|e| panic!("lower {rel} ({profile}): {e}"));
        let mut heap = Heap::new();
        let report = counted.execute_with_bridge(&mut heap, FUEL, HostMode::LiveOs, bridge);
        let outcome_desc = match &report.outcome {
            ProgramOutcome::Failed(cause) => {
                let parts: Vec<String> = heap
                    .cause_reasons(cause)
                    .expect("cause reasons")
                    .map(|r| match r {
                        Reason::Die(v) => {
                            format!("Die({:?})", heap.defect(v).map(|d| d.code().clone()))
                        }
                        Reason::Fail(v) => format!("Fail({v:?})"),
                        Reason::Interrupt { fiber_id } => {
                            format!("Interrupt({fiber_id:?})")
                        }
                    })
                    .collect();
                format!("Failed([{}])", parts.join(", "))
            }
            other => format!("{other:?}"),
        };
        assert_eq!(
            report.stdout, expected_stdout,
            "stdout mismatch for {rel} ({profile}); outcome = {outcome_desc}"
        );
        if expect_fail {
            match report.outcome {
                ProgramOutcome::Failed(cause) => {
                    if let Some(needle) = &stderr_contains {
                        let needle = needle.trim();
                        let reasons: Vec<_> = heap
                            .cause_reasons(&cause)
                            .expect("cause reasons")
                            .map(|r| format!("{r:?}"))
                            .collect();
                        let msg = heap
                            .cause_reasons(&cause)
                            .expect("cause reasons")
                            .filter_map(|r| match r {
                                Reason::Die(v) => {
                                    heap.defect(v).ok().map(|d| format!("{:?}", d.code()))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("; ");
                        assert!(
                            msg.contains(needle) || reasons.iter().any(|s| s.contains(needle)),
                            "expected defect containing `{needle}` for {rel} ({profile}), got `{msg}` / `{reasons:?}`"
                        );
                    }
                    heap.drop_cause(cause).expect("drop failure cause");
                }
                ProgramOutcome::Raised(val) => {
                    heap.drop_value(val).expect("drop raised payload");
                    panic!("expected defect/Failed for {rel} ({profile}), got Raised");
                }
                ProgramOutcome::Returned(val) => {
                    let _ = heap.drop_value(val);
                    panic!("expected failure for {rel} ({profile}), got Returned");
                }
                ProgramOutcome::Fault(err) => {
                    panic!("unexpected evaluator fault for {rel} ({profile}): {err}");
                }
            }
        } else {
            match report.outcome {
                ProgramOutcome::Returned(val) => {
                    assert_eq!(val, Value::Unit, "non-unit return for {rel} ({profile})");
                    heap.drop_value(val).expect("drop return");
                }
                other => {
                    let _ = other.release(&mut heap);
                    panic!("expected Returned(()) for {rel} ({profile})");
                }
            }
        }
        heap.finish()
            .unwrap_or_else(|e| panic!("Rust heap leak for {rel} ({profile}): {e}"));
    }
}

#[test]
fn stage5_ffi_fixtures_run_with_zero_rust_and_host_leaks() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in [
        "tests/run/ffi/foreign.fl",
        "tests/run/ffi/copy.fl",
        "tests/run/ffi/events.fl",
        "tests/run/ffi/snapshot.fl",
        "tests/run/ffi/catch_order.fl",
        "tests/run/ffi/defect.fl",
    ] {
        run_bridged_fixture(&mut bridge, rel);
    }
}

#[test]
fn stage5_rt_owner_foreign_fixtures_run_with_zero_leaks() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in [
        "tests/rt/owner_channel.fl",
        "tests/rt/owner_foreign_policy.fl",
        "tests/rt/owner_foreign_repeat.fl",
    ] {
        run_bridged_fixture(&mut bridge, rel);
    }
}

#[test]
fn stage5_first_slice_example_runs_with_zero_leaks() {
    let dir = repo_path("examples/first-slice/src/App");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read examples/first-slice/src/App")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("fl"))
        .collect();
    files.sort();
    let modules = compile_files(&files).expect("compile examples/first-slice");
    let main = main_of(&modules).expect("main of first-slice");
    let mut bridge = HostBridge::new().expect("spawn HostBridge");

    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main_with_bridge(&modules, &main, release)
            .unwrap_or_else(|e| panic!("lower first-slice ({profile}): {e}"));
        let mut heap = Heap::new();
        let report = counted.execute_with_bridge(&mut heap, FUEL, HostMode::LiveOs, &mut bridge);
        match report.outcome {
            ProgramOutcome::Returned(val) => {
                assert_eq!(val, Value::Unit);
                heap.drop_value(val).expect("drop return");
            }
            other => {
                let _ = other.release(&mut heap);
                panic!("first-slice ({profile}) failed");
            }
        }
        assert!(
            !report.stdout.is_empty(),
            "first-slice ({profile}) produced empty stdout"
        );
        heap.finish()
            .unwrap_or_else(|e| panic!("first-slice ({profile}) heap leak: {e}"));
    }
}

#[test]
fn stage5_ffi_fuel_prefix_sweeps_have_zero_leaks_on_both_heaps() {
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    for rel in ["tests/run/ffi/foreign.fl", "tests/run/ffi/snapshot.fl"] {
        let path = repo_path(rel);
        let modules = compile(&path).expect("compile");
        let main = main_of(&modules).expect("main");
        let counted = counted_main_with_bridge(&modules, &main, false).expect("lower");
        for fuel in [0, 1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500] {
            let mut heap = Heap::new();
            let report =
                counted.execute_with_bridge(&mut heap, fuel, HostMode::LiveOs, &mut bridge);
            match report.outcome {
                ProgramOutcome::Fault(MemoryError::LeakedForeignHandles(n)) => {
                    panic!("leaked {n} host handles at fuel={fuel} for {rel}");
                }
                other => {
                    other.release(&mut heap).expect("release outcome");
                }
            }
            heap.finish()
                .unwrap_or_else(|e| panic!("Rust heap leak at fuel={fuel} for {rel}: {e}"));
        }
    }
}
