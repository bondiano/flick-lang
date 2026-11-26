//! M3 `Ref` reference-cycle policy tests:
//!
//! Pins the documented Phase-M policy (docs/formal/memory.md §5,
//! spec/05-errors-and-concurrency.md, reference/regions.md):
//! - Plain immutable ADTs/records/closures form an acyclic counted ownership graph
//!   and reclaim immediately with `cells_detached_on_teardown == 0`.
//! - Cycles through `Ref` (direct self-cycle, mutual cycle, `Ref`-through-closure,
//!   `Ref`-through-ADT/record) are supported deterministically via root-arena
//!   cell ownership: payloads stay live until overwritten or execution teardown,
//!   then detach-all teardown reclaims 100% of cyclic slots with zero leaks.
//! - Breaking a `Ref` cycle via `Ref.set` before drop immediately reclaims the old
//!   cyclic payload via normal RC drops before execution teardown.
//! - Frame-scoped `let mut` reclaims on frame exit with zero root-arena `Ref` cells.
//! - External aliases to values stored inside a cyclic `Ref` survive teardown
//!   untouched and drop cleanly afterward.

use flick_core::ir::Lit;
use flick_model::run::{CountedMain, compile_source, counted_main, main_of};
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Stats, Value};

mod raw_ir;

fn run_source_both_profiles(tag: &str, source: &str, expected_stdout: &str) -> (Stats, Stats) {
    let modules = compile_source(source).unwrap_or_else(|e| panic!("{tag}: compile failed: {e}"));
    let entry = main_of(&modules).unwrap_or_else(|e| panic!("{tag}: missing main: {e}"));
    let mut results = Vec::new();
    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let counted = counted_main(&modules, &entry, release)
            .unwrap_or_else(|e| panic!("{tag} ({profile}): lower failed: {e}"));
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, 2_000_000);
        let returned_unit = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{:?}", report.outcome);
        report
            .outcome
            .release(&mut heap)
            .unwrap_or_else(|e| panic!("{tag} ({profile}): release failed: {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("{tag} ({profile}): heap leak: {e}"));
        assert!(
            returned_unit,
            "{tag} ({profile}): expected Returned(()), got {detail}; stdout={:?}",
            report.stdout
        );
        assert_eq!(
            report.stdout, expected_stdout,
            "{tag} ({profile}): stdout mismatch"
        );
        assert_eq!(stats.live_objects, 0, "{tag} ({profile}) live_objects");
        assert_eq!(
            stats.allocations, stats.frees,
            "{tag} ({profile}) alloc/free"
        );
        assert_eq!(
            stats.cells_allocated, stats.cells_freed,
            "{tag} ({profile}) cells alloc/free"
        );
        results.push(stats);
    }
    (results[0], results[1])
}

fn sweep_source_prefixes(tag: &str, source: &str, expected_stdout: &str, dense_limit: u64) {
    let modules = compile_source(source).unwrap_or_else(|e| panic!("{tag}: compile failed: {e}"));
    let entry = main_of(&modules).unwrap_or_else(|e| panic!("{tag}: missing main: {e}"));
    let counted: CountedMain = counted_main(&modules, &entry, false)
        .unwrap_or_else(|e| panic!("{tag}: lower failed: {e}"));

    let mut completed = false;
    let mut saw_oof = false;
    let mut fuel = 0u64;
    while fuel <= dense_limit || !completed {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external-cycle-root".into()))
            .expect("external root");
        let report = counted.execute(&mut heap, fuel);
        match report.outcome {
            ProgramOutcome::Returned(v) => {
                completed = true;
                assert_eq!(v, Value::Unit, "{tag} fuel={fuel}");
                assert_eq!(report.stdout, expected_stdout, "{tag} fuel={fuel}");
                heap.drop_value(v).expect("drop unit");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {
                saw_oof = true;
                assert!(
                    expected_stdout.starts_with(&report.stdout),
                    "{tag} fuel={fuel}: stdout {:?} not prefix of {:?}",
                    report.stdout,
                    expected_stdout
                );
            }
            other => {
                other.release(&mut heap).expect("release unexpected");
                panic!("{tag} fuel={fuel}: unexpected non-OutOfFuel outcome");
            }
        }
        assert_eq!(
            heap.string(&external),
            Ok("external-cycle-root"),
            "{tag} fuel={fuel}: external alias corrupted"
        );
        assert_eq!(
            heap.stats().live_objects,
            1,
            "{tag} fuel={fuel}: VM leaked object on cycle teardown"
        );
        assert_eq!(
            heap.stats().cells_allocated,
            heap.stats().cells_freed,
            "{tag} fuel={fuel}: leaked cell slot"
        );
        heap.drop_value(external).expect("drop external");
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("{tag} fuel={fuel}: heap not empty: {e}"));
        assert_eq!(stats.allocations, stats.frees, "{tag} fuel={fuel}");
        if completed && fuel >= dense_limit {
            break;
        }
        if fuel < dense_limit {
            fuel += 1;
        } else {
            fuel = fuel.saturating_mul(2).max(fuel + 64);
            assert!(fuel <= 500_000, "{tag}: never completed within 500k fuel");
        }
    }
    assert!(completed, "{tag}: never reached completion");
    assert!(saw_oof, "{tag}: never exercised OutOfFuel");
}

#[test]
fn direct_self_cycle_through_ref_reclaims_cleanly_at_teardown_and_every_fuel_prefix() {
    let source = r#"
module App.Main exposing (main)

type SelfNode = SelfNode String (Ref (Option SelfNode))

main : Unit ->{Host.Console} Unit
let main () =
  let slot : Ref (Option SelfNode) = Ref.make None
  let node = SelfNode "self-node" slot
  Ref.set slot (Some node)
  match Ref.get slot with
  | Some (SelfNode label next) ->
      match Ref.get next with
      | Some (SelfNode label2 _) -> Platform.stdout_line (label ++ "->" ++ label2)
      | None -> Platform.stdout_line "broken"
  | None -> Platform.stdout_line "empty"
"#;
    let expected = "self-node->self-node\n";
    let (debug_stats, release_stats) =
        run_source_both_profiles("direct_self_cycle", source, expected);
    for (profile, stats) in [("debug", debug_stats), ("release", release_stats)] {
        assert!(
            stats.cells_allocated >= 1,
            "{profile}: expected allocated Ref cell"
        );
        assert!(
            stats.cells_detached_on_teardown >= 1,
            "{profile}: unbroken self-cycle must be detached on root-arena teardown"
        );
    }
    sweep_source_prefixes("direct_self_cycle", source, expected, 400);
}

#[test]
fn mutual_ref_cycle_reclaims_cleanly_at_teardown_and_every_fuel_prefix() {
    let source = r#"
module App.Main exposing (main)

type Peer = Peer String (Ref (Option Peer))

main : Unit ->{Host.Console} Unit
let main () =
  let ra : Ref (Option Peer) = Ref.make None
  let rb : Ref (Option Peer) = Ref.make None
  let a = Peer "alpha" ra
  let b = Peer "beta" rb
  Ref.set ra (Some b)
  Ref.set rb (Some a)
  let out =
    match Ref.get ra with
    | Some (Peer n1 next1) ->
        match Ref.get next1 with
        | Some (Peer n2 _) -> n1 ++ "<->" ++ n2
        | None -> "missing-second"
    | None -> "missing-first"
  Platform.stdout_line out
"#;
    let expected = "beta<->alpha\n";
    let (debug_stats, release_stats) =
        run_source_both_profiles("mutual_ref_cycle", source, expected);
    for (profile, stats) in [("debug", debug_stats), ("release", release_stats)] {
        assert!(
            stats.cells_allocated >= 2,
            "{profile}: expected both Ref cells allocated"
        );
        assert!(
            stats.cells_detached_on_teardown >= 2,
            "{profile}: unbroken mutual cycle must detach both cell slots at teardown"
        );
    }
    sweep_source_prefixes("mutual_ref_cycle", source, expected, 500);
}

#[test]
fn ref_through_closure_and_record_cycles_reclaim_cleanly_at_teardown() {
    let source = r#"
module App.Main exposing (main)

type Env = Env { tag : String; step : Ref (Int ->{} Int) }

init_fn : Int ->{} Int
let init_fn n = n

main : Unit ->{Host.Console} Unit
let main () =
  let fref : Ref (Int ->{} Int) = Ref.make init_fn
  Ref.set fref (fun n -> if n <= 0 then 0 else n + (Ref.get fref) (n - 1))
  Platform.stdout_line ("closure-sum=" ++ show ((Ref.get fref) 5))

  let step : Ref (Int ->{} Int) = Ref.make init_fn
  let env = Env { tag = "rec"; step = step }
  Ref.set step (fun n ->
    match env with
    | Env r ->
        if n <= 0 then 1
        else n * (Ref.get r.step) (n - 1)
  )
  match env with
  | Env r -> Platform.stdout_line (r.tag ++ "-fact=" ++ show ((Ref.get r.step) 5))
"#;
    let expected = "closure-sum=15\nrec-fact=120\n";
    let (debug_stats, release_stats) =
        run_source_both_profiles("ref_closure_record_cycle", source, expected);
    for (profile, stats) in [("debug", debug_stats), ("release", release_stats)] {
        assert!(stats.cells_allocated >= 2, "{profile}");
        assert!(
            stats.cells_detached_on_teardown >= 2,
            "{profile}: closure/record Ref cycles must detach at teardown"
        );
    }
    sweep_source_prefixes("ref_closure_record_cycle", source, expected, 600);
}

#[test]
fn broken_cycle_via_overwrite_reclaims_old_cyclic_payload_immediately_before_teardown() {
    let source = r#"
module App.Main exposing (main)

type SelfNode = SelfNode String (Ref (Option SelfNode))

main : Unit ->{Host.Console} Unit
let main () =
  let slot : Ref (Option SelfNode) = Ref.make None
  let node = SelfNode "ephemeral" slot
  Ref.set slot (Some node)
  let before =
    match Ref.get slot with
    | Some (SelfNode s _) -> s
    | None -> "none"
  Ref.set slot None
  let after =
    match Ref.get slot with
    | Some (SelfNode s _) -> s
    | None -> "cleared"
  Platform.stdout_line (before ++ ":" ++ after)
"#;
    let expected = "ephemeral:cleared\n";
    run_source_both_profiles("broken_cycle_source", source, expected);
    sweep_source_prefixes("broken_cycle_source", source, expected, 400);
}

#[test]
fn frame_scoped_let_mut_cycle_and_acyclic_corpus_boundary_are_measured_separately() {
    let let_mut_source = r"
module App.Main exposing (main)

run_loop : Int ->{} Int
let run_loop n =
  let mut acc = 0
  let mut i = n
  while i > 0 do {
    acc = acc + i
    i = i - 1
  }
  acc

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (run_loop 10))
";
    let (lm_debug, lm_release) =
        run_source_both_profiles("frame_scoped_let_mut", let_mut_source, "55\n");
    for (profile, stats) in [("debug", lm_debug), ("release", lm_release)] {
        assert_eq!(
            stats.cells_allocated, 0,
            "{profile}: frame-scoped `let mut` must not allocate root-arena `Ref` cells"
        );
        assert_eq!(
            stats.cells_detached_on_teardown, 0,
            "{profile}: frame-scoped `let mut` reclaims strictly at frame exit"
        );
    }

    let acyclic_source = r"
module App.Main exposing (main)

type IntList = Nil | Cons Int IntList

build : Int -> IntList ->{} IntList
let rec build n acc =
  if n <= 0 then acc else build (n - 1) (Cons n acc)

inc_all : IntList ->{} IntList
let rec inc_all xs =
  match xs with
  | Nil -> Nil
  | Cons h t -> Cons (h + 1) (inc_all t)

sum : IntList -> Int ->{} Int
let rec sum xs acc =
  match xs with
  | Nil -> acc
  | Cons h t -> sum t (acc + h)

main : Unit ->{Host.Console} Unit
let main () =
  let xs = build 20 Nil
  let ys = inc_all xs
  Platform.stdout_line (show (sum ys 0))
";
    let (ac_debug, ac_release) =
        run_source_both_profiles("acyclic_reclamation_boundary", acyclic_source, "230\n");
    for (profile, stats) in [("debug", ac_debug), ("release", ac_release)] {
        assert_eq!(
            stats.cells_allocated, 0,
            "{profile}: pure acyclic workload allocates zero Ref cells"
        );
        assert_eq!(
            stats.cells_detached_on_teardown, 0,
            "{profile}: pure acyclic workload must reclaim 100% via Perceus RC without detach-all"
        );
        assert!(
            stats.reuses >= 20,
            "{profile}: expected FBIP reuse on acyclic inc_all, got {}",
            stats.reuses
        );
    }
}
