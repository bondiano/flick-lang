//! M2 regressions for overlapping `MemoMap` abort+rebuild consumers and
//! failure-exit propagation across failing `tail` / `control` arms.

use super::memory_m2_arenas::assert_source_and_fuel_sweep;

const MEMO_ABORT_REBUILD_OVERLAPPING_SOURCE: &str = r#"
module App.Main exposing (main)

import Arena

effect Halt =
  abort halt : forall a. String ->{} a

service Box =
  label : Unit ->{} String

build_exited :
  Fiber.Provided {Host.Console, Async, Halt} s =>
  Layer.MemoMap -> Layer {Host.Console, Async, Halt} none {Box} ->{Scope s, Host.Console, Async, Halt} Exit none (Layer.Context {Box})
let build_exited m l = Effect.exit (task { Layer.build_with_memo_map m l })

main : Unit ->{Host.Console, Async} Unit
let main () =
  let memo = Layer.make_memo_map ()
  let attempts = Ref.make 0
  let fin2_ran = Ref.make false
  let build1_entered = Deferred.make ()
  let waiter_ready = Deferred.make ()
  let c1_aborted = Deferred.make ()
  let c2_built = Deferred.make ()
  let waiter_closed = Deferred.make ()

  let shared_layer =
    Layer.from_build_memo
      (fun _m ->
        let n = Ref.get attempts + 1
        Ref.set attempts n
        let a = Arena.make ()
        let v = Arena.alloc a ("build-" ++ show n)
        Scope.add_finalizer
          (task {
            if n == 2 then Ref.set fin2_ran true else ()
            Host.raw_stdout_line ("fin-" ++ show n ++ " " ++ Arena.get v)
          })
        if n == 1 then
          let _ = Deferred.succeed () build1_entered
          let _ = Deferred.await_ waiter_ready
          Effect.yield_now ()
          Effect.yield_now ()
          Halt.halt "abort-build-1"
        else
          Context.make Box { label = fun () -> "ok-2" }
      )

  scope root {
    let f1 =
      Fiber.spawn
        root
        (fun _ -> ())
        (task {
          let res =
            try
              scope {
                let _ = Layer.build_with_memo_map memo shared_layer
                "unexpected"
              }
            with
            | Halt.halt msg -> "c1:" ++ msg
          Host.raw_stdout_line res
          let _ = Deferred.succeed () c1_aborted
          ()
        })

    let f_waiter =
      Fiber.spawn
        root
        (fun _ -> ())
        (task {
          let _ =
            try
              scope {
                let _ = Deferred.await_ build1_entered
                let _ = Deferred.succeed () waiter_ready
                let ex = build_exited memo shared_layer
                Host.raw_stdout_line ("waiter fail=" ++ show (Exit.is_failure ex))
                let _ = Deferred.await_ c2_built
                ()
              }
            with
            | Halt.halt _ -> ()
          let _ = Deferred.succeed () waiter_closed
          ()
        })

    let _ = Deferred.await_ c1_aborted
    let _ =
      try
        scope {
          let _ctx = Layer.build_with_memo_map memo shared_layer
          let _ = Deferred.succeed () c2_built
          let _ = Deferred.await_ waiter_closed
          Host.raw_stdout_line ("c2-live fin2=" ++ show (Ref.get fin2_ran))
        }
      with
      | Halt.halt _ -> ()
    Host.raw_stdout_line ("after-c2 fin2=" ++ show (Ref.get fin2_ran))
    let _ = Fiber.join f1
    let _ = Fiber.join f_waiter
    ()
  }
"#;

#[test]
fn memo_abort_rebuild_overlapping_consumers_preserves_replacement_arena() {
    let expected = "\
fin-1 build-1
c1:abort-build-1
waiter fail=true
c2-live fin2=false
fin-2 build-2
after-c2 fin2=true
";
    assert_source_and_fuel_sweep(
        "memo_abort_rebuild_overlapping",
        MEMO_ABORT_REBUILD_OVERLAPPING_SOURCE,
        expected,
        2,
        2500,
    );
}

const FAILING_TAIL_EXIT_CAUSE_SOURCE: &str = r#"
module App.Main exposing (main)

import Arena

type error SigErr = SigErr String

effect Ask =
  tail ask : Unit ->{} String

effect Ctrl =
  control step : Unit ->{} String

service LayerSvc =
  tag : Unit ->{} String

main : Unit ->{Host.Console} Unit
let main () =
  // 1. Failing `tail` arm across Scope.add_finalizer_exit, Scope.defer,
  //    Resource.make, Effect.on_exit, and a provided Layer.from_build scope.
  let provided_layer =
    Layer.from_build
      (fun _m ->
        let la = Arena.make ()
        let lv = Arena.alloc la "prov-arena"
        Scope.defer
          (fun ex ->
            Host.raw_stdout_line
              ("prov-defer fail=" ++ show (Exit.is_failure ex) ++ " " ++ Arena.get lv)
          )
        Context.make LayerSvc { tag = fun () -> "svc" }
      )

  let tail_res =
    try
      try
        scope {
          let a = Arena.make ()
          let v = Arena.alloc a "tail-arena"
          Scope.add_finalizer_exit
            (fun c ->
              Host.raw_stdout_line
                ("tail-fin-exit some=" ++ show (Option.is_some c) ++ " " ++ Arena.get v)
            )
          Scope.defer
            (fun ex ->
              Host.raw_stdout_line
                ("tail-defer fail=" ++ show (Exit.is_failure ex) ++ " " ++ Arena.get v)
            )
          let _r =
            Resource.make
              (task { "res-1" })
              (fun r ex ->
                Host.raw_stdout_line
                  ("tail-res-make " ++ r ++ " fail=" ++ show (Exit.is_failure ex) ++ " " ++ Arena.get v)
              )
          provide provided_layer {
            let _ = LayerSvc.tag ()
            Effect.on_exit
              (fun ex ->
                Host.raw_stdout_line
                  ("tail-on-exit fail=" ++ show (Exit.is_failure ex) ++ " " ++ Arena.get v)
              )
              (task {
                let _ = Ask.ask ()
                "never"
              })
          }
        }
      with
      | Ask.ask () -> Fail.fail (SigErr "tail-boom")
    with
    | SigErr m -> "tail-recovered:" ++ m
  Host.raw_stdout_line tail_res

  // 2. Failing `tail` arm during a partial Layer.from_build acquisition.
  let partial_layer =
    Layer.from_build
      (fun _m ->
        let la = Arena.make ()
        let lv = Arena.alloc la "build-arena"
        Scope.defer
          (fun ex ->
            Host.raw_stdout_line
              ("build-defer fail=" ++ show (Exit.is_failure ex) ++ " " ++ Arena.get lv)
          )
        let _ = Ask.ask ()
        Context.make LayerSvc { tag = fun () -> "never" }
      )

  let build_res =
    try
      try
        provide partial_layer { "never" }
      with
      | Ask.ask () -> Fail.fail (SigErr "build-tail-boom")
    with
    | SigErr m -> "build-recovered:" ++ m
  Host.raw_stdout_line build_res

  // 3. Failing `control` arm discontinuing an inner scope with arena.
  let ctrl_res =
    try
      try
        scope {
          let a = Arena.make ()
          let v = Arena.alloc a "ctrl-arena"
          Scope.add_finalizer_exit
            (fun c ->
              Host.raw_stdout_line
                ("ctrl-fin-exit some=" ++ show (Option.is_some c) ++ " " ++ Arena.get v)
            )
          Scope.defer
            (fun ex ->
              Host.raw_stdout_line
                ("ctrl-defer fail=" ++ show (Exit.is_failure ex) ++ " " ++ Arena.get v)
            )
          let _ = Ctrl.step ()
          "never"
        }
      with
      | Ctrl.step () -> Fail.fail (SigErr "ctrl-boom")
    with
    | SigErr m -> "ctrl-recovered:" ++ m
  Host.raw_stdout_line ctrl_res
"#;

#[test]
fn failing_tail_and_control_arms_propagate_failure_exit_to_scopes_and_on_exit() {
    let expected = "\
tail-on-exit fail=true tail-arena
prov-defer fail=true prov-arena
tail-res-make res-1 fail=true tail-arena
tail-defer fail=true tail-arena
tail-fin-exit some=true tail-arena
tail-recovered:tail-boom
build-defer fail=true build-arena
build-recovered:build-tail-boom
ctrl-defer fail=true ctrl-arena
ctrl-fin-exit some=true ctrl-arena
ctrl-recovered:ctrl-boom
";
    assert_source_and_fuel_sweep(
        "failing_tail_exit_cause",
        FAILING_TAIL_EXIT_CAUSE_SOURCE,
        expected,
        4,
        2500,
    );
}
