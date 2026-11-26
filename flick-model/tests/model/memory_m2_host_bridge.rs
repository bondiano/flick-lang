//! M2 `HostBridge` async ownership: C4 pending promise settlement with live arena
//! reads and C5 finalizer registration, plus late `Js.promise_callback`
//! rejection with `RuntimeFailure` and zero-handle-leak fuel-prefix sweeps.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use flick_model::run::{compile, counted_main_with_bridge, main_of};
use flick_vm::tier0::{Error, Heap, HostBridge, HostMode, ProgramOutcome, Value};

struct TempFixture {
    dir: PathBuf,
}

impl TempFixture {
    fn new(tag: &str) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("target/tmp")
            .join(format!("m2-bridge-{tag}-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        Self { dir }
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.dir.join(rel);
        fs::write(&path, content).expect("write fixture file");
        path
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const LATE_CB_JS: &str = r#"
let savedPromiseCb = null;

export function savePromiseCb(cb) {
  savedPromiseCb = cb;
}

export function startTrackedPromiseCb(arg) {
  return savedPromiseCb(arg);
}

export async function invokeSavedPromiseCbLate(arg) {
  try {
    const val = await savedPromiseCb(arg);
    return `unexpected:${val}`;
  } catch (err) {
    return `rejected:${err?.name ?? "Error"}`;
  }
}

export function runInFlightPromiseCb(cb) {
  // Start the promise callback immediately while the scope is still open;
  // it finishes asynchronously after `race_first` cancels this branch so C4
  // must await it before C5 finalizers and C6 arena retirement.
  return cb(undefined);
}
"#;

const LATE_CB_FL: &str = r#"
module App.Main exposing (main)

import Arena
import Js

save_promise_cb : (Js.PromiseCallback String String)^c ->{} Unit
foreign effect save_promise_cb from "./late_cb.mjs" "savePromiseCb"
  retention call

start_tracked_promise_cb : String ->{Async} String
foreign start_tracked_promise_cb from "./late_cb.mjs" "startTrackedPromiseCb"

invoke_saved_promise_cb_late : String ->{Async} String
foreign invoke_saved_promise_cb_late from "./late_cb.mjs" "invokeSavedPromiseCbLate"

run_in_flight_promise_cb : (Js.PromiseCallback Unit String)^c ->{Async} String
foreign run_in_flight_promise_cb from "./late_cb.mjs" "runInFlightPromiseCb"
  retention call

nap : Float ->{Async} Unit
let nap ms =
  match Duration.millis ms with
  | Ok d -> Effect.sleep d
  | Err _ -> ()

main : Unit ->{Host.Console, Async} Unit
let main () =
  // 1. In-flight tracked promise callback settling during C4 sees live arena AND registers a C5 finalizer
  let _ =
    scope {
      let a = Arena.make ()
      let v = Arena.alloc a "c4-arena"
      let cb =
        Js.promise_callback
          (fun () ->
            nap 15
            let got = Arena.get v
            Scope.add_finalizer (task { Host.raw_stdout_line ("c5-from-c4 " ++ Arena.get v) })
            Host.raw_stdout_line ("c4-read " ++ got)
            got
          )
      let winner =
        race_first
          (task { run_in_flight_promise_cb cb })
          (task {
            nap 1
            "fast"
          })
      Host.raw_stdout_line ("race " ++ winner)
    }
  Host.raw_stdout_line "after-c4-scope"

  // 2. Js.promise_callback: open call succeeds while scope is open; late call after close rejects cleanly
  let _ =
    scope {
      let a = Arena.make ()
      let v = Arena.alloc a "pcb-live"
      let cb = Js.promise_callback (fun tag -> tag ++ ":" ++ Arena.get v)
      save_promise_cb cb
      let live_res = start_tracked_promise_cb "open"
      Host.raw_stdout_line ("pcb " ++ live_res)
    }
  let late_pcb = invoke_saved_promise_cb_late "after"
  Host.raw_stdout_line ("late-pcb " ++ late_pcb)
"#;

#[test]
fn host_bridge_c4_pending_async_and_late_callbacks_honor_arena_lifetime() {
    let tmp = TempFixture::new("late_cb");
    tmp.write("late_cb.mjs", LATE_CB_JS);
    let file = tmp.write("Main.fl", LATE_CB_FL);

    let modules = compile(&file).expect("compile host bridge late_cb");
    let entry = main_of(&modules).expect("main");
    let mut bridge = HostBridge::new().expect("spawn HostBridge");
    let expected = "\
race fast
c4-read c4-arena
c5-from-c4 c4-arena
after-c4-scope
pcb open:pcb-live
late-pcb rejected:RuntimeFailure
";

    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main_with_bridge(&modules, &entry, release)
            .unwrap_or_else(|e| panic!("lower ({profile}): {e}"));
        let mut heap = Heap::new();
        let report =
            counted.execute_with_bridge(&mut heap, 5_000_000, HostMode::LiveOs, &mut bridge);
        let outcome_dbg = format!("{:?}", report.outcome);
        let returned_unit = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        report
            .outcome
            .release(&mut heap)
            .unwrap_or_else(|e| panic!("release ({profile}): {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("Rust heap leak ({profile}): {e}"));
        assert_eq!(stats.live_objects, 0, "{profile} live_objects");
        assert_eq!(
            stats.arenas_allocated, stats.arenas_freed,
            "{profile} arenas_allocated != arenas_freed"
        );
        assert!(
            returned_unit,
            "expected Returned(()) ({profile}), got {outcome_dbg}; stdout={:?}",
            report.stdout
        );
        assert_eq!(report.stdout, expected, "stdout ({profile})");
    }

    // Fuel-prefix sweep over bridged execution verifying zero Rust & host handle leaks
    let counted = counted_main_with_bridge(&modules, &entry, false).expect("lower debug");
    for fuel in [0, 1, 10, 50, 200, 1000] {
        let mut heap = Heap::new();
        let report = counted.execute_with_bridge(&mut heap, fuel, HostMode::LiveOs, &mut bridge);
        match report.outcome {
            ProgramOutcome::Fault(Error::LeakedForeignHandles(n)) => {
                panic!("leaked {n} host handles at fuel={fuel}");
            }
            ProgramOutcome::Fault(Error::HostBridge(msg)) => {
                panic!("unexpected HostBridge error at fuel={fuel}: {msg}");
            }
            other => other.release(&mut heap).expect("release outcome"),
        }
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("Rust heap leak at fuel={fuel}: {e}"));
        assert_eq!(stats.arenas_allocated, stats.arenas_freed, "fuel={fuel}");
    }
}
