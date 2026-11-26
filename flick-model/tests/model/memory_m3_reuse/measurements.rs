//! M3 counted-VM allocation, reuse, peak-live-object, and cycle measurements (issue #462 / #869).
//!
//! Reports identical-input comparisons across all four required scenario classes:
//! 1. Unique receivers (ADT constructors & records),
//! 2. Deliberately shared receivers (with independent alias preservation checks),
//! 3. Exceptional / early exit (with LIFO finalizer order observations),
//! 4. `Ref` cycles (with documented `cells_detached_on_teardown` outcomes).
//!
//! Execution engine for every runtime counter: `flick-model` counted VM
//! (`flick-vm/src/tier0/`). Static reuse sites are counted directly on the
//! `flick-core` Perceus-lowered memory IR (`Operation::Reuse* | Update`).
//! Peak live bytes, physical target RSS, and target speed are explicitly
//! `unmeasured` (no Wasm/Native RC codegen backend exists in M3).

use super::compare_fbip_vs_baseline;

struct MeasuredWorkload {
    class: &'static str,
    name: &'static str,
    source: &'static str,
    expected_stdout: &'static str,
    min_static_sites: usize,
    min_reuses: usize,
    min_fallbacks: usize,
    min_cells_detached: usize,
    max_reuses: Option<usize>,
}

const WORKLOADS: &[MeasuredWorkload] = &[
    MeasuredWorkload {
        class: "unique",
        name: "list_map (N=100)",
        source: r"
module App.Main exposing (main)
type IntList = Nil | Cons Int IntList
build : Int -> IntList ->{} IntList
let rec build n acc = if n <= 0 then acc else build (n - 1) (Cons n acc)
map_inc : IntList ->{} IntList
let rec map_inc xs = match xs with | Nil -> Nil | Cons h t -> Cons (h + 1) (map_inc t)
sum : IntList -> Int ->{} Int
let rec sum xs acc = match xs with | Nil -> acc | Cons h t -> sum t (acc + h)
main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line (show (sum (map_inc (build 100 Nil)) 0))
",
        expected_stdout: "5150\n",
        min_static_sites: 2,
        min_reuses: 100,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "unique",
        name: "list_reverse (N=100)",
        source: r"
module App.Main exposing (main)
type IntList = Nil | Cons Int IntList
build : Int -> IntList ->{} IntList
let rec build n acc = if n <= 0 then acc else build (n - 1) (Cons n acc)
rev_into : IntList -> IntList ->{} IntList
let rec rev_into xs acc = match xs with | Nil -> acc | Cons h t -> rev_into t (Cons h acc)
sum : IntList -> Int ->{} Int
let rec sum xs acc = match xs with | Nil -> acc | Cons h t -> sum t (acc + h)
main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line (show (sum (rev_into (build 100 Nil) Nil) 0))
",
        expected_stdout: "5050\n",
        min_static_sites: 1,
        min_reuses: 100,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "unique",
        name: "list_filter (N=100, 50% kept)",
        source: r"
module App.Main exposing (main)
type IntList = Nil | Cons Int IntList
build : Int -> IntList ->{} IntList
let rec build n acc = if n <= 0 then acc else build (n - 1) (Cons n acc)
filter_even : IntList ->{} IntList
let rec filter_even xs =
  match xs with
  | Nil -> Nil
  | Cons h t -> if h % 2 == 0 then Cons h (filter_even t) else filter_even t
sum : IntList -> Int ->{} Int
let rec sum xs acc = match xs with | Nil -> acc | Cons h t -> sum t (acc + h)
main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line (show (sum (filter_even (build 100 Nil)) 0))
",
        expected_stdout: "2550\n",
        min_static_sites: 2,
        min_reuses: 50,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "unique",
        name: "tree_map (depth=6, 63 nodes)",
        source: r"
module App.Main exposing (main)
type Tree = Leaf | Node Tree Int Tree
mk : Int -> Int ->{} Tree
let rec mk d v = if d <= 0 then Leaf else Node (mk (d - 1) (v * 2)) v (mk (d - 1) (v * 2 + 1))
tmap : Tree ->{} Tree
let rec tmap t = match t with | Leaf -> Leaf | Node l v r -> Node (tmap l) (v + 1) (tmap r)
tsum : Tree ->{} Int
let rec tsum t = match t with | Leaf -> 0 | Node l v r -> v + tsum l + tsum r
main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line (show (tsum (tmap (mk 6 1))))
",
        expected_stdout: "2079\n",
        min_static_sites: 2,
        min_reuses: 63,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "unique",
        name: "bst_insert (N=40 inserts)",
        source: r"
module App.Main exposing (main)
type Tree = Leaf | Node Tree Int Tree
insert : Int -> Tree ->{} Tree
let rec insert k t =
  match t with
  | Leaf -> Node Leaf k Leaf
  | Node l v r ->
      if k < v then Node (insert k l) v r
      else if k > v then Node l v (insert k r)
      else Node l v r
fill : Int -> Tree ->{} Tree
let rec fill n t = if n <= 0 then t else fill (n - 1) (insert ((n * 17) % 40) t)
tcount : Tree ->{} Int
let rec tcount t = match t with | Leaf -> 0 | Node l _ r -> 1 + tcount l + tcount r
main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line (show (tcount (fill 40 Leaf)))
",
        expected_stdout: "40\n",
        min_static_sites: 3,
        min_reuses: 100,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "unique",
        name: "record_update (N=100 steps)",
        source: r#"
module App.Main exposing (main)
type alias Point = { x : Int; y : Int }
step : Int -> Point ->{} Point
let rec step n p = if n <= 0 then p else step (n - 1) { p with x = p.x + 1; y = p.y + 2 }
main : Unit ->{Host.Console} Unit
let main () =
  let p = step 100 { x = 0; y = 0 }
  Platform.stdout_line (show p.x ++ "," ++ show p.y)
"#,
        expected_stdout: "100,200\n",
        min_static_sites: 1,
        min_reuses: 100,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "unique",
        name: "record_rebuild (N=100 steps)",
        source: r#"
module App.Main exposing (main)
type alias Point = { x : Int; y : Int }
step : Int -> Point ->{} Point
let rec step n p =
  if n <= 0 then p
  else
    let { x; y } = p
    step (n - 1) { x = x + 1; y = y + 3 }
main : Unit ->{Host.Console} Unit
let main () =
  let p = step 100 { x = 0; y = 0 }
  Platform.stdout_line (show p.x ++ "," ++ show p.y)
"#,
        expected_stdout: "100,300\n",
        min_static_sites: 1,
        min_reuses: 100,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "shared",
        name: "list_map_shared (N=50, alias checked)",
        source: r#"
module App.Main exposing (main)
type IntList = Nil | Cons Int IntList
build : Int -> IntList ->{} IntList
let rec build n acc = if n <= 0 then acc else build (n - 1) (Cons n acc)
map_inc : IntList ->{} IntList
let rec map_inc xs = match xs with | Nil -> Nil | Cons h t -> Cons (h + 1) (map_inc t)
sum : IntList -> Int ->{} Int
let rec sum xs acc = match xs with | Nil -> acc | Cons h t -> sum t (acc + h)
main : Unit ->{Host.Console} Unit
let main () =
  let orig = build 50 Nil
  let mapped = map_inc orig
  Platform.stdout_line ("orig=" ++ show (sum orig 0) ++ ";mapped=" ++ show (sum mapped 0))
"#,
        expected_stdout: "orig=1275;mapped=1325\n",
        min_static_sites: 2,
        min_reuses: 0,
        min_fallbacks: 51,
        min_cells_detached: 0,
        max_reuses: Some(0),
    },
    MeasuredWorkload {
        class: "shared",
        name: "record_update_shared (N=50, alias checked)",
        source: r#"
module App.Main exposing (main)
type alias Point = { x : Int; y : Int }
bump_shared : Int -> Point -> Int ->{} Int
let rec bump_shared n orig acc =
  if n <= 0 then acc
  else
    let next = { orig with x = orig.x + n; y = orig.y + 1 }
    bump_shared (n - 1) orig (acc + next.x + next.y)
main : Unit ->{Host.Console} Unit
let main () =
  let orig = { x = 10; y = 20 }
  let total = bump_shared 50 orig 0
  Platform.stdout_line ("orig=" ++ show orig.x ++ "," ++ show orig.y ++ ";total=" ++ show total)
"#,
        expected_stdout: "orig=10,20;total=2825\n",
        min_static_sites: 1,
        min_reuses: 0,
        min_fallbacks: 50,
        min_cells_detached: 0,
        max_reuses: Some(0),
    },
    MeasuredWorkload {
        class: "early_exit",
        name: "fallible_map_lifo_finalizers (N=20, fail@10)",
        source: r#"
module App.Main exposing (main)
type IntList = Nil | Cons Int IntList
type error BadItem = BadItem Int
build : Int -> IntList ->{} IntList
let rec build n acc = if n <= 0 then acc else build (n - 1) (Cons n acc)
map_fallible : IntList ->{; BadItem} IntList
let rec map_fallible xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      if h == 10 then Fail.fail (BadItem h)
      else Cons (h * 10) (map_fallible t)
main : Unit ->{Host.Console} Unit
let main () =
  let res =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-1" })
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-2" })
        let _ = map_fallible (build 20 Nil)
        "unreachable"
      }
    with
    | BadItem n -> "caught:" ++ show n
  Host.raw_stdout_line res
"#,
        expected_stdout: "fin-2\nfin-1\ncaught:10\n",
        min_static_sites: 2,
        min_reuses: 0,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: Some(0),
    },
    MeasuredWorkload {
        class: "early_exit",
        name: "fallible_rev_lifo_finalizers (N=20, fail@10)",
        source: r#"
module App.Main exposing (main)
type IntList = Nil | Cons Int IntList
type error BadItem = BadItem Int
build : Int -> IntList ->{} IntList
let rec build n acc = if n <= 0 then acc else build (n - 1) (Cons n acc)
rev_fallible : IntList -> IntList ->{; BadItem} IntList
let rec rev_fallible xs acc =
  match xs with
  | Nil -> acc
  | Cons h t ->
      if h == 10 then Fail.fail (BadItem h)
      else rev_fallible t (Cons (h * 10) acc)
main : Unit ->{Host.Console} Unit
let main () =
  let res =
    try
      scope {
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-1" })
        Scope.add_finalizer (task { Host.raw_stdout_line "fin-2" })
        let _ = rev_fallible (build 20 Nil) Nil
        "unreachable"
      }
    with
    | BadItem n -> "caught:" ++ show n
  Host.raw_stdout_line res
"#,
        expected_stdout: "fin-2\nfin-1\ncaught:10\n",
        min_static_sites: 1,
        min_reuses: 9,
        min_fallbacks: 0,
        min_cells_detached: 0,
        max_reuses: None,
    },
    MeasuredWorkload {
        class: "cycle",
        name: "ref_self_and_mutual_teardown (3 cells)",
        source: r#"
module App.Main exposing (main)
type SelfNode = SelfNode String (Ref (Option SelfNode))
type Peer = Peer String (Ref (Option Peer))
main : Unit ->{Host.Console} Unit
let main () =
  let s : Ref (Option SelfNode) = Ref.make None
  Ref.set s (Some (SelfNode "self" s))
  let ra : Ref (Option Peer) = Ref.make None
  let rb : Ref (Option Peer) = Ref.make None
  Ref.set ra (Some (Peer "beta" rb))
  Ref.set rb (Some (Peer "alpha" ra))
  let s_out =
    match Ref.get s with
    | Some (SelfNode a n) ->
        match Ref.get n with
        | Some (SelfNode b _) -> a ++ "->" ++ b
        | None -> "none"
    | None -> "none"
  let m_out =
    match Ref.get ra with
    | Some (Peer a n) ->
        match Ref.get n with
        | Some (Peer b _) -> a ++ "<->" ++ b
        | None -> "none"
    | None -> "none"
  Platform.stdout_line (s_out ++ ";" ++ m_out)
"#,
        expected_stdout: "self->self;beta<->alpha\n",
        min_static_sites: 0,
        min_reuses: 0,
        min_fallbacks: 0,
        min_cells_detached: 3,
        max_reuses: Some(0),
    },
];

#[test]
fn m3_allocation_and_reuse_measurements() {
    println!(
        "| Class | Workload | Profile | Engine | Static reuse sites | Eligible attempts | Runtime reuses | Fallbacks | Baseline allocs | FBIP allocs | Saved | Base peak objs | FBIP peak objs | Peak bytes | Cells detached |"
    );
    println!("| - | - | - | - | -: | -: | -: | -: | -: | -: | -: | -: | -: | - | -: |");
    for w in WORKLOADS {
        let (dbg, rel) = compare_fbip_vs_baseline(w.name, w.source, w.expected_stdout);
        for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
            let attempts = cmp.fbip.reuses + cmp.fbip.reuse_fallbacks;
            let saved = cmp
                .baseline
                .allocations
                .saturating_sub(cmp.fbip.allocations);
            println!(
                "| {} | {} | {profile} | flick-model counted VM | {} | {} | {} | {} | {} | {} | {} | {} | {} | unmeasured | {} |",
                w.class,
                w.name,
                cmp.static_reuse_sites,
                attempts,
                cmp.fbip.reuses,
                cmp.fbip.reuse_fallbacks,
                cmp.baseline.allocations,
                cmp.fbip.allocations,
                saved,
                cmp.baseline.peak_live_objects,
                cmp.fbip.peak_live_objects,
                cmp.fbip.cells_detached_on_teardown,
            );
            assert!(
                cmp.static_reuse_sites >= w.min_static_sites,
                "{} ({profile}): static_reuse_sites={} < {}",
                w.name,
                cmp.static_reuse_sites,
                w.min_static_sites
            );
            assert!(
                cmp.fbip.reuses >= w.min_reuses,
                "{} ({profile}): reuses={} < {}",
                w.name,
                cmp.fbip.reuses,
                w.min_reuses
            );
            if let Some(max) = w.max_reuses {
                assert!(
                    cmp.fbip.reuses <= max,
                    "{} ({profile}): reuses={} > {max}",
                    w.name,
                    cmp.fbip.reuses
                );
            }
            assert!(
                cmp.fbip.reuse_fallbacks >= w.min_fallbacks,
                "{} ({profile}): fallbacks={} < {}",
                w.name,
                cmp.fbip.reuse_fallbacks,
                w.min_fallbacks
            );
            assert!(
                cmp.fbip.allocations + w.min_reuses <= cmp.baseline.allocations,
                "{} ({profile}): expected >= {} allocations saved (baseline={}, fbip={})",
                w.name,
                w.min_reuses,
                cmp.baseline.allocations,
                cmp.fbip.allocations
            );
            assert!(
                cmp.fbip.cells_detached_on_teardown >= w.min_cells_detached,
                "{} ({profile}): cells_detached_on_teardown={} < {}",
                w.name,
                cmp.fbip.cells_detached_on_teardown,
                w.min_cells_detached
            );
            assert!(
                cmp.fbip.peak_live_objects > 0 && cmp.baseline.peak_live_objects > 0,
                "{} ({profile}): expected non-zero peak_live_objects",
                w.name
            );
        }
    }
}
