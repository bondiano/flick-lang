//! M2 scoped arenas and owner cleanup across Signals, failure recovery,
//! cancellation/children, persistent contexts, and dynamic/memo layers —
//! including exhaustive fuel-prefix zero-live-object sweeps.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use flick_core::ir::Lit;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value};
use flick_model::run::{CountedMain, compile, counted_main, main_of};

struct TempFixture {
    dir: PathBuf,
}

impl TempFixture {
    fn new(tag: &str) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("target/tmp")
            .join(format!("m2-arenas-{tag}-{}", process::id()));
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

fn sweep_fuel_prefixes(tag: &str, counted: &CountedMain, expected_stdout: &str, dense_limit: u64) {
    let mut completed = false;
    let mut saw_oob = false;
    let mut fuel = 0u64;
    while fuel <= dense_limit || !completed {
        let mut h = Heap::new();
        let external = h
            .literal(&Lit::String("external-caller-root".into()))
            .expect("external root");
        let r = counted.execute(&mut h, fuel);
        let detail = format!(
            "{tag} fuel={fuel}: outcome={:?} stdout={:?}",
            r.outcome, r.stdout
        );
        match r.outcome {
            ProgramOutcome::Returned(v) => {
                completed = true;
                assert_eq!(v, Value::Unit, "{detail}");
                assert_eq!(r.stdout, expected_stdout, "{detail}");
                h.drop_value(v).expect("drop unit");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {
                saw_oob = true;
                assert!(
                    expected_stdout.starts_with(&r.stdout),
                    "{detail}: prefix stdout {:?} not prefix of {:?}",
                    r.stdout,
                    expected_stdout
                );
            }
            other => {
                other.release(&mut h).expect("release unexpected outcome");
                panic!("{detail}: unexpected non-OutOfFuel outcome");
            }
        }
        assert_eq!(h.string(&external), Ok("external-caller-root"), "{detail}");
        assert_eq!(h.stats().live_objects, 1, "{detail}: leaked VM object");
        assert_eq!(
            h.stats().arenas_allocated,
            h.stats().arenas_freed,
            "{detail}: leaked arena object on fuel cut"
        );
        h.drop_value(external).expect("drop external root");
        let st = h
            .finish()
            .unwrap_or_else(|e| panic!("{detail}: heap not empty: {e}"));
        assert_eq!(st.allocations, st.frees, "{detail}");
        if completed && fuel >= dense_limit {
            break;
        }
        if fuel < dense_limit {
            fuel += 1;
        } else {
            fuel = fuel.saturating_mul(2).max(fuel + 100);
            assert!(fuel <= 500_000, "{tag}: never completed within 500k fuel");
        }
    }
    assert!(completed, "{tag}: sweep never reached completion");
    assert!(saw_oob, "{tag}: sweep never exercised OutOfFuel prefixes");
}

/// Runs `source` in debug and release profiles, checking exact expected stdout,
/// zero live objects, equal allocations/frees, and equal arenas allocated/freed.
/// Then sweeps every fuel prefix `0..=dense_limit` (and exponentially to completion)
/// while keeping an independent external caller root alive, asserting zero live
/// arena-owned objects and zero leaks on every single prefix.
pub(crate) fn assert_source_and_fuel_sweep(
    tag: &str,
    source: &str,
    expected_stdout: &str,
    min_arenas: usize,
    dense_limit: u64,
) {
    let tmp = TempFixture::new(tag);
    let file = tmp.write("Main.fl", source);
    let modules = compile(&file).unwrap_or_else(|e| panic!("{tag}: compile failed: {e}"));
    let entry = main_of(&modules).unwrap_or_else(|e| panic!("{tag}: main export missing: {e}"));

    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main(&modules, &entry, release)
            .unwrap_or_else(|e| panic!("{tag} ({profile}): lower failed: {e}"));

        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, 2_000_000);
        let outcome_dbg = format!("{:?}", report.outcome);
        let returned_unit = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        report
            .outcome
            .release(&mut heap)
            .unwrap_or_else(|e| panic!("{tag} ({profile}): release failed: {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("{tag} ({profile}): heap leak: {e}"));
        assert!(
            returned_unit,
            "{tag} ({profile}): expected Returned(()), got {outcome_dbg}; stdout={:?}",
            report.stdout
        );
        assert_eq!(
            report.stdout, expected_stdout,
            "{tag} ({profile}): stdout mismatch"
        );
        assert_eq!(stats.live_objects, 0, "{tag} ({profile}) live_objects");
        assert_eq!(
            stats.allocations, stats.frees,
            "{tag} ({profile}) allocations != frees"
        );
        assert_eq!(
            stats.arenas_allocated, stats.arenas_freed,
            "{tag} ({profile}) arenas_allocated != arenas_freed"
        );
        assert!(
            stats.arenas_allocated >= min_arenas,
            "{tag} ({profile}): expected at least {min_arenas} arenas, got {}",
            stats.arenas_allocated
        );

        if !release {
            sweep_fuel_prefixes(tag, &counted, expected_stdout, dense_limit);
        }
    }
}

#[test]
fn tail_arm_failure_runs_inner_scope_finalizers_and_retires_arena() {
    let source = r#"
module App.Main exposing (main)

import Arena

type error Boom = Boom String

effect Ask =
  ask : Unit ->{} Int

main : Unit ->{Host.Console} Unit
let main () =
  let res =
    try
      try
        scope {
          let read = Ref.make (fun () -> Platform.stdout_line "unset")
          Scope.add_finalizer (task { (Ref.get read) () })
          let arena = Arena.make ()
          let view = Arena.alloc arena "tail-live"
          Ref.set read (fun () -> Platform.stdout_line ("first " ++ Arena.get view))
          Scope.add_finalizer (task { Platform.stdout_line ("last " ++ Arena.get view) })
          let inner =
            try
              show (Ask.ask ())
            with
            | Boom _ -> "wrong-inner-catch"
          inner
        }
      with
      | Ask.ask () -> Fail.fail (Boom "from-tail")
    with
    | Boom msg -> "caught " ++ msg
  Platform.stdout_line res
"#;
    let expected = "last tail-live\nfirst tail-live\ncaught from-tail\n";
    assert_source_and_fuel_sweep("tail_fail", source, expected, 1, 1500);
}

const SIGNALS_ABORT_CONTROL_SOURCE: &str = r#"
module App.Main exposing (main)

import Arena

type error ArmErr = ArmErr String

effect Escape =
  abort escape : forall a. String ->{} a

effect Yield =
  control yield_ : Int ->{} Int

main : Unit ->{Host.Console} Unit
let main () =
  // 1. abort unwinds nested scopes with arenas + LIFO finalizers; outer arena survives inner close
  let aborted =
    scope {
      let outer_a = Arena.make ()
      let outer_v = Arena.alloc outer_a "outer"
      let out =
        try
          scope {
            let r = Ref.make (fun () -> Platform.stdout_line "unset")
            Scope.add_finalizer (task { (Ref.get r) () })
            let inner_a = Arena.make ()
            let inner_v = Arena.alloc inner_a "inner"
            Ref.set r (fun () -> Platform.stdout_line ("abort-fin1 " ++ Arena.get inner_v))
            Scope.add_finalizer (task { Platform.stdout_line ("abort-fin2 " ++ Arena.get inner_v) })
            Escape.escape ("via-" ++ Arena.get outer_v)
          }
        with
        | Escape.escape msg -> "escaped:" ++ msg
      Platform.stdout_line (out ++ ":" ++ Arena.get outer_v)
    }
  ignore aborted

  // 2. control arm resuming once keeps inner arena live across resume, then closes LIFO
  let resumed =
    try
      scope {
        let a = Arena.make ()
        let v = Arena.alloc a "resumed"
        Scope.add_finalizer (task { Platform.stdout_line ("ctrl-res-fin " ++ Arena.get v) })
        let n = Yield.yield_ 10
        Arena.get v ++ ":" ++ show n
      }
    with
    | Yield.yield_ x -> resume (x + 5)
  Platform.stdout_line resumed

  // 3. control arm returning without resuming discontinues scope, running LIFO finalizers before arena close
  let disc =
    try
      scope {
        let a = Arena.make ()
        let v = Arena.alloc a "disc"
        Scope.add_finalizer (task { Platform.stdout_line ("ctrl-disc-fin1 " ++ Arena.get v) })
        Scope.add_finalizer (task { Platform.stdout_line ("ctrl-disc-fin2 " ++ Arena.get v) })
        let _ = Yield.yield_ 99
        "unreachable"
      }
    with
    | Yield.yield_ x -> "dropped:" ++ show x
  Platform.stdout_line disc

  // 4. control arm failing without resuming still discontinues inner arena scope cleanly
  let ctrl_fail =
    try
      try
        scope {
          let a = Arena.make ()
          let v = Arena.alloc a "ctrl-fail"
          Scope.add_finalizer (task { Platform.stdout_line ("ctrl-fail-fin " ++ Arena.get v) })
          let _ = Yield.yield_ 1
          "unreachable"
        }
      with
      | Yield.yield_ _ -> Fail.fail (ArmErr "arm-boom")
    with
    | ArmErr m -> "ctrl-caught:" ++ m
  Platform.stdout_line ctrl_fail

  // 5. resume twice raises defect and closes arena scope cleanly without leaks
  let ex : Exit none String =
    Effect.exit
      (task {
        try
          scope {
            let a = Arena.make ()
            let v = Arena.alloc a "twice"
            Scope.add_finalizer (task { Platform.stdout_line ("twice-fin " ++ Arena.get v) })
            let n = Yield.yield_ 3
            show n
          }
        with
        | Yield.yield_ x -> resume x ++ resume x
      })
  Platform.stdout_line ("twice-die:" ++ show (Exit.has_dies ex))
"#;

#[test]
fn signals_abort_and_control_close_arena_scopes_in_lifo_order() {
    let expected = "\
abort-fin2 inner
abort-fin1 inner
escaped:via-outer:outer
ctrl-res-fin resumed
resumed:15
ctrl-disc-fin2 disc
ctrl-disc-fin1 disc
dropped:99
ctrl-fail-fin ctrl-fail
ctrl-caught:arm-boom
twice-fin twice
twice-die:true
";
    assert_source_and_fuel_sweep(
        "signals_abort_control",
        SIGNALS_ABORT_CONTROL_SOURCE,
        expected,
        6,
        2000,
    );
}

#[test]
fn signal_abort_and_control_during_layer_build_close_partial_layer_finalizers_and_arenas() {
    let source = r#"
module App.Main exposing (main)

import Arena

service Svc =
  name : Unit ->{} String

effect Escape =
  abort escape : forall a. String ->{} a

effect Yield =
  control yield_ : String ->{} String

main : Unit ->{Host.Console} Unit
let main () =
  // 1. abort inside Layer.from_build closes partial layer's arena and runs its LIFO finalizers
  let abort_layer =
    Layer.from_build
      (fun _memo ->
        let a = Arena.make ()
        let v = Arena.alloc a "layer-abort"
        Scope.add_finalizer (task { Host.raw_stdout_line ("layer-abort-fin1 " ++ Arena.get v) })
        Scope.add_finalizer (task { Host.raw_stdout_line ("layer-abort-fin2 " ++ Arena.get v) })
        let _ : Unit = Escape.escape "aborted-build"
        Context.make Svc { name = fun () -> "never" }
      )
  let r1 =
    try
      provide abort_layer { Svc.name () }
    with
    | Escape.escape msg -> msg
  Platform.stdout_line r1

  // 2. control discontinue inside Layer.from_build_memo closes partial layer's arena and runs finalizer
  let ctrl_layer =
    Layer.from_build_memo
      (fun _memo ->
        let a = Arena.make ()
        let v = Arena.alloc a "layer-ctrl"
        Scope.add_finalizer (task { Host.raw_stdout_line ("layer-ctrl-fin " ++ Arena.get v) })
        let _ = Yield.yield_ "stop-build"
        Context.make Svc { name = fun () -> "never" }
      )
  let r2 =
    try
      provide ctrl_layer { Svc.name () }
    with
    | Yield.yield_ msg -> msg
  Platform.stdout_line r2
"#;
    let expected = "\
layer-abort-fin2 layer-abort
layer-abort-fin1 layer-abort
aborted-build
layer-ctrl-fin layer-ctrl
stop-build
";
    assert_source_and_fuel_sweep("signal_layer_build", source, expected, 2, 1500);
}

const RECOVERY_AND_CANCELLATION_SOURCE: &str = r#"
module App.Main exposing (main)

import Arena

type error FailE = FailE String

nap : Float ->{Async} Unit
let nap ms =
  match Duration.millis ms with
  | Ok d -> Effect.sleep d
  | Err _ -> ()

run_child_scope : Unit ->{Async} Unit
let run_child_scope () =
  scope owner {
    let a = Arena.make ()
    let v = Arena.alloc a "parent-arena"
    Scope.add_finalizer (task { Host.raw_stdout_line ("parent-fin " ++ Arena.get v) })
    let ready : Deferred.Deferred none Unit = Deferred.make ()
    let _child =
      Fiber.spawn
        owner
        (fun f -> Host.raw_stdout_line ("sup-int:" ++ show (Cause.has_interrupts f)))
        (task {
          Effect.ensuring
            (task { Host.raw_stdout_line ("child-cleanup " ++ Arena.get v) })
            (task {
              ignore (Deferred.succeed () ready)
              Effect.never ()
            })
        })
    Deferred.await_ ready
    Host.raw_stdout_line "parent-body"
  }

main : Unit ->{Host.Console, Async} Unit
let main () =
  // 1. Failure recovery + nested scope + late arena allocated inside finalizer
  let r1 =
    try
      scope {
        let a = Arena.make ()
        let v = Arena.alloc a "fail-scope"
        Scope.add_finalizer
          (task {
            scope {
              let late = Arena.make ()
              let lv = Arena.alloc late "late-in-fin"
              Host.raw_stdout_line ("fin-fail " ++ Arena.get v ++ "/" ++ Arena.get lv)
            }
          })
        Fail.fail (FailE "boom")
      }
    with
    | FailE m -> "recovered:" ++ m
  Host.raw_stdout_line r1

  // 2. Child fiber spawned into scope reads arena during interrupt cleanup BEFORE parent LIFO finalizer
  run_child_scope ()

  // 3. parallel failing sibling cancels arena-holding sibling cleanly
  let p_res =
    try
      let _ =
        parallel
          (task {
            scope {
              let a = Arena.make ()
              let v = Arena.alloc a "par-loser"
              Scope.add_finalizer (task { Host.raw_stdout_line ("par-fin " ++ Arena.get v) })
              nap 5000
              "slow"
            }
          })
          (task {
            nap 5
            Fail.fail (FailE "fast-err")
          })
      "ok"
    with
    | FailE m -> "par-caught:" ++ m
  Host.raw_stdout_line p_res

  // 4. race_first cancels loser scope with arena + timeout cancels inner arena scope
  let winner =
    race_first
      (task {
        scope {
          let a = Arena.make ()
          let v = Arena.alloc a "race-loser"
          Scope.add_finalizer (task { Host.raw_stdout_line ("race-fin " ++ Arena.get v) })
          nap 5000
          "slow"
        }
      })
      (task {
        nap 5
        "race-win"
      })
  Host.raw_stdout_line winner

  let timed =
    try
      timeout
        5
        (task {
          scope {
            let a = Arena.make ()
            let v = Arena.alloc a "timed"
            Scope.add_finalizer (task { Host.raw_stdout_line ("timeout-fin " ++ Arena.get v) })
            nap 5000
            "never"
          }
        })
    with
    | TimeoutError -> "timed-out"
  Host.raw_stdout_line timed
"#;

#[test]
fn failure_recovery_and_cancellation_leave_zero_live_arena_objects_at_every_fuel_prefix() {
    let expected = "\
fin-fail fail-scope/late-in-fin
recovered:boom
parent-body
child-cleanup parent-arena
sup-int:true
parent-fin parent-arena
par-fin par-loser
par-caught:fast-err
race-fin race-loser
race-win
timeout-fin timed
timed-out
";
    assert_source_and_fuel_sweep(
        "recovery_and_cancellation",
        RECOVERY_AND_CANCELLATION_SOURCE,
        expected,
        6,
        2500,
    );
}

#[test]
fn persistent_contexts_and_dynamic_memo_layers_transport_owner_evidence_cleanly() {
    let source = r#"
module App.Main exposing (main)

import Arena

service TagA =
  tag_a : Unit ->{} String

service TagB =
  tag_b : Unit ->{} String

alloc_outer : String -> Unit ->{Scope s} Arena.Value s String
let alloc_outer label () =
  let a = Arena.make ()
  let v = Arena.alloc a label
  Scope.add_finalizer (task { Host.raw_stdout_line ("outer-fin " ++ Arena.get v) })
  v

main : Unit ->{Host.Console} Unit
let main () =
  let pref = Context.reference "m2/prefix" (fun () -> "default")
  scope {
    // Persistent Context built via Context.make/add/merge + Layer.succeed_context
    let c1 = Context.make TagA { tag_a = fun () -> "A" }
    let c2 = Context.make TagB { tag_b = fun () -> "B" }
    let merged = Context.merge c2 c1
    let ctx_layer = Layer.succeed_context merged

    // Memoized dynamic layer acquiring its own arena + finalizer; copied-out value serves TagA
    let builds = Ref.make 0
    let shared_layer =
      Layer.from_build_memo
        (fun _memo ->
          Ref.set builds (Ref.get builds + 1)
          let la = Arena.make ()
          let lv = Arena.alloc la ("memo-" ++ show (Ref.get builds))
          let copied = Arena.get lv
          Scope.add_finalizer (task { Host.raw_stdout_line ("memo-fin " ++ Arena.get lv) })
          Context.make TagA { tag_a = fun () -> copied }
        )

    let outer_v =
      Context.with_reference
        pref
        "custom"
        (task {
          provide ctx_layer {
            let p = Context.read pref
            let label = p ++ ":" ++ TagA.tag_a () ++ "+" ++ TagB.tag_b ()
            // Acquire into outer `scope`, NOT the provider's private owner
            alloc_outer label ()
          }
        })
    // Outer arena view survives ctx_layer close
    Platform.stdout_line ("after-ctx " ++ Arena.get outer_v)

    // Memoized layer overridden/shared twice builds once and cleans up its arena once
    let pair_layer = Layer.override shared_layer shared_layer
    provide pair_layer {
      Platform.stdout_line ("memo-val " ++ TagA.tag_a ())
    }
    Platform.stdout_line ("builds " ++ show (Ref.get builds))
  }
"#;
    let expected = "\
after-ctx custom:A+B
memo-val memo-1
memo-fin memo-1
builds 1
outer-fin custom:A+B
";
    assert_source_and_fuel_sweep("persistent_contexts", source, expected, 2, 2000);
}
