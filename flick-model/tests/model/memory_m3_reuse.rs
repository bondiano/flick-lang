//! M3 safe FBIP / in-place reuse verification suite (shared harness + constructor reuse).

use flick_core::ir::{
    Lit,
    memory::{Block, Exit, Instruction, Operation, Program},
};
use flick_model::memory::{Error, Heap, ProgramOutcome, Stats, Value};
use flick_model::run::{
    CountedMain, compile_source, counted_main, counted_main_without_reuse, main_of,
};

mod effects_and_benchmarks;
mod measurements;
mod records_and_matches;

pub(super) struct Comparison {
    pub baseline: Stats,
    pub fbip: Stats,
    pub static_reuse_sites: usize,
}

fn count_block_reuse_sites(block: &Block) -> usize {
    let mut count = block
        .instructions
        .iter()
        .filter(|inst| {
            matches!(
                inst,
                Instruction::Let {
                    value: Operation::Reuse { .. }
                        | Operation::ReuseWithDicts { .. }
                        | Operation::ReuseRecord { .. }
                        | Operation::Update { .. },
                    ..
                }
            )
        })
        .count();
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        count += count_block_reuse_sites(then_) + count_block_reuse_sites(else_);
    }
    count
}

fn count_program_reuse_sites(program: &Program) -> usize {
    program
        .functions
        .values()
        .map(|f| count_block_reuse_sites(&f.body))
        .sum()
}

fn run_once(
    tag: &str,
    counted: &CountedMain,
    enable_heap_reuse: bool,
    expected_stdout: &str,
) -> Stats {
    let mut heap = if enable_heap_reuse {
        Heap::new()
    } else {
        Heap::without_reuse()
    };
    let report = counted.execute(&mut heap, 5_000_000);
    let returned_unit = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
    let detail = format!("{:?}", report.outcome);
    report
        .outcome
        .release(&mut heap)
        .unwrap_or_else(|e| panic!("{tag}: release failed: {e}"));
    let stats = heap
        .finish()
        .unwrap_or_else(|e| panic!("{tag}: heap leak: {e}"));
    assert!(
        returned_unit,
        "{tag}: expected Returned(()), got {detail}; stdout={:?}",
        report.stdout
    );
    assert_eq!(report.stdout, expected_stdout, "{tag}: stdout mismatch");
    assert_eq!(stats.live_objects, 0, "{tag}: live_objects != 0");
    assert_eq!(
        stats.allocations, stats.frees,
        "{tag}: allocations != frees"
    );
    assert_eq!(
        stats.cells_allocated, stats.cells_freed,
        "{tag}: cells_allocated != cells_freed"
    );
    assert_eq!(
        stats.arenas_allocated, stats.arenas_freed,
        "{tag}: arenas_allocated != arenas_freed"
    );
    stats
}

pub(super) fn compare_fbip_vs_baseline(
    tag: &str,
    source: &str,
    expected_stdout: &str,
) -> (Comparison, Comparison) {
    let modules = compile_source(source).unwrap_or_else(|e| panic!("{tag}: compile failed: {e}"));
    let entry = main_of(&modules).unwrap_or_else(|e| panic!("{tag}: main missing: {e}"));
    let mut out = Vec::new();
    for release in [false, true] {
        let profile = if release { "release" } else { "debug" };
        let baseline_prog = counted_main_without_reuse(&modules, &entry, release)
            .unwrap_or_else(|e| panic!("{tag} ({profile} baseline): lower failed: {e}"));
        let fbip_prog = counted_main(&modules, &entry, release)
            .unwrap_or_else(|e| panic!("{tag} ({profile} fbip): lower failed: {e}"));

        let baseline = run_once(
            &format!("{tag} ({profile} baseline)"),
            &baseline_prog,
            false,
            expected_stdout,
        );
        let fbip = run_once(
            &format!("{tag} ({profile} fbip)"),
            &fbip_prog,
            true,
            expected_stdout,
        );
        assert_eq!(
            baseline.reuses, 0,
            "{tag} ({profile} baseline): baseline must perform zero reuses"
        );
        let static_reuse_sites = count_program_reuse_sites(&fbip_prog.program);
        out.push(Comparison {
            baseline,
            fbip,
            static_reuse_sites,
        });
    }
    let release_cmp = out.pop().expect("release comparison");
    let debug_cmp = out.pop().expect("debug comparison");
    (debug_cmp, release_cmp)
}

pub(super) fn sweep_fuel_prefixes(
    tag: &str,
    source: &str,
    expected_stdout: &str,
    dense_limit: u64,
) {
    let modules = compile_source(source).unwrap_or_else(|e| panic!("{tag}: compile failed: {e}"));
    let entry = main_of(&modules).unwrap_or_else(|e| panic!("{tag}: main missing: {e}"));
    let counted = counted_main(&modules, &entry, false)
        .unwrap_or_else(|e| panic!("{tag}: lower failed: {e}"));

    let mut completed = false;
    let mut saw_oof = false;
    let mut fuel = 0u64;
    while fuel <= dense_limit || !completed {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external-fbip-root".into()))
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
                    "{tag} fuel={fuel}: prefix stdout {:?} not prefix of {:?}",
                    report.stdout,
                    expected_stdout
                );
            }
            other => {
                other.release(&mut heap).expect("release unexpected");
                panic!("{tag} fuel={fuel}: unexpected outcome");
            }
        }
        assert_eq!(
            heap.string(&external),
            Ok("external-fbip-root"),
            "{tag} fuel={fuel}: external alias corrupted"
        );
        assert_eq!(
            heap.stats().live_objects,
            1,
            "{tag} fuel={fuel}: leaked VM object on FBIP fuel cut"
        );
        heap.drop_value(external).expect("drop external");
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("{tag} fuel={fuel}: heap not empty: {e}"));
        assert_eq!(stats.allocations, stats.frees, "{tag} fuel={fuel}");
        assert_eq!(
            stats.cells_allocated, stats.cells_freed,
            "{tag} fuel={fuel}"
        );
        assert_eq!(
            stats.arenas_allocated, stats.arenas_freed,
            "{tag} fuel={fuel}"
        );
        if completed && fuel >= dense_limit {
            break;
        }
        if fuel < dense_limit {
            fuel += 1;
        } else {
            fuel = fuel.saturating_mul(2).max(fuel + 64);
            assert!(fuel <= 1_000_000, "{tag}: never completed within 1M fuel");
        }
    }
    assert!(completed, "{tag}: never completed");
    assert!(saw_oof, "{tag}: never exercised OutOfFuel");
}

const UNIQUE_LIST_PIPELINE_SOURCE: &str = r"
module App.Main exposing (main)

type IntList = Nil | Cons Int IntList

build : Int -> IntList ->{} IntList
let rec build n acc =
  if n <= 0 then acc else build (n - 1) (Cons n acc)

map_inc : IntList ->{} IntList
let rec map_inc xs =
  match xs with
  | Nil -> Nil
  | Cons h t -> Cons (h + 1) (map_inc t)

rev_into : IntList -> IntList ->{} IntList
let rec rev_into xs acc =
  match xs with
  | Nil -> acc
  | Cons h t -> rev_into t (Cons h acc)

filter_even : IntList ->{} IntList
let rec filter_even xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      if h % 2 == 0 then Cons h (filter_even t)
      else filter_even t

sum : IntList -> Int ->{} Int
let rec sum xs acc =
  match xs with
  | Nil -> acc
  | Cons h t -> sum t (acc + h)

main : Unit ->{Host.Console} Unit
let main () =
  let xs = build 20 Nil
  let ys = map_inc xs
  let zs = rev_into ys Nil
  let evens = filter_even zs
  Platform.stdout_line (show (sum evens 0))
";

const SHARED_LIST_PIPELINE_SOURCE: &str = r#"
module App.Main exposing (main)

type IntList = Nil | Cons Int IntList

build : Int -> IntList ->{} IntList
let rec build n acc =
  if n <= 0 then acc else build (n - 1) (Cons n acc)

map_inc : IntList ->{} IntList
let rec map_inc xs =
  match xs with
  | Nil -> Nil
  | Cons h t -> Cons (h + 1) (map_inc t)

rev_into : IntList -> IntList ->{} IntList
let rec rev_into xs acc =
  match xs with
  | Nil -> acc
  | Cons h t -> rev_into t (Cons h acc)

filter_even : IntList ->{} IntList
let rec filter_even xs =
  match xs with
  | Nil -> Nil
  | Cons h t ->
      if h % 2 == 0 then Cons h (filter_even t)
      else filter_even t

to_csv : IntList ->{} String
let rec to_csv xs =
  match xs with
  | Nil -> ""
  | Cons h Nil -> show h
  | Cons h t -> show h ++ "," ++ to_csv t

main : Unit ->{Host.Console} Unit
let main () =
  let orig = build 5 Nil
  let mapped = map_inc orig
  let reversed = rev_into orig Nil
  let evens = filter_even orig
  Platform.stdout_line ("orig=" ++ to_csv orig)
  Platform.stdout_line ("mapped=" ++ to_csv mapped)
  Platform.stdout_line ("reversed=" ++ to_csv reversed)
  Platform.stdout_line ("evens=" ++ to_csv evens)
"#;

#[test]
fn constructor_reuse_list_map_reverse_filter_unique_and_shared_fallback() {
    let (dbg, rel) =
        compare_fbip_vs_baseline("list_pipeline_unique", UNIQUE_LIST_PIPELINE_SOURCE, "110\n");
    for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
        assert!(
            cmp.fbip.reuses >= 50,
            "{profile}: expected >= 50 constructor reuses, got {}",
            cmp.fbip.reuses
        );
        assert!(
            cmp.fbip.allocations + 50 <= cmp.baseline.allocations,
            "{profile}: expected >= 50 fewer allocations (baseline={}, fbip={})",
            cmp.baseline.allocations,
            cmp.fbip.allocations
        );
    }
    sweep_fuel_prefixes(
        "list_pipeline_unique",
        UNIQUE_LIST_PIPELINE_SOURCE,
        "110\n",
        400,
    );

    let shared_expected = "orig=1,2,3,4,5\nmapped=2,3,4,5,6\nreversed=5,4,3,2,1\nevens=2,4\n";
    let (s_dbg, s_rel) = compare_fbip_vs_baseline(
        "list_pipeline_shared",
        SHARED_LIST_PIPELINE_SOURCE,
        shared_expected,
    );
    for (profile, cmp) in [("debug", &s_dbg), ("release", &s_rel)] {
        assert!(
            cmp.fbip.reuse_fallbacks >= 10,
            "{profile}: shared inputs must trigger dynamic fallback (got {})",
            cmp.fbip.reuse_fallbacks
        );
    }
    sweep_fuel_prefixes(
        "list_pipeline_shared",
        SHARED_LIST_PIPELINE_SOURCE,
        shared_expected,
        400,
    );
}

#[test]
fn constructor_reuse_tree_map_and_bst_insert_unique_and_shared() {
    let source = r#"
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

tree_map : Tree ->{} Tree
let rec tree_map t =
  match t with
  | Leaf -> Leaf
  | Node l v r -> Node (tree_map l) (v * 10) (tree_map r)

inorder : Tree ->{} String
let rec inorder t =
  match t with
  | Leaf -> ""
  | Node l v r ->
      let ls = inorder l
      let rs = inorder r
      let p1 = if ls == "" then show v else ls ++ "," ++ show v
      if rs == "" then p1 else p1 ++ "," ++ rs

main : Unit ->{Host.Console} Unit
let main () =
  let t0 = Leaf
  let t1 = insert 5 t0
  let t2 = insert 3 t1
  let t3 = insert 8 t2
  let t4 = insert 1 t3
  let t5 = insert 4 t4
  let t6 = insert 7 t5
  let t7 = insert 9 t6
  let mapped_shared = tree_map t7
  let inserted_shared = insert 6 t7
  Platform.stdout_line ("orig=" ++ inorder t7)
  Platform.stdout_line ("mapped=" ++ inorder mapped_shared)
  Platform.stdout_line ("ins6=" ++ inorder inserted_shared)
  let mapped_unique = tree_map mapped_shared
  Platform.stdout_line ("mapped2=" ++ inorder mapped_unique)
"#;
    let expected = "\
orig=1,3,4,5,7,8,9
mapped=10,30,40,50,70,80,90
ins6=1,3,4,5,6,7,8,9
mapped2=100,300,400,500,700,800,900
";
    let (dbg, rel) = compare_fbip_vs_baseline("tree_map_bst_insert", source, expected);
    for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
        assert!(
            cmp.fbip.reuses >= 15,
            "{profile}: expected >= 15 tree node reuses, got {}",
            cmp.fbip.reuses
        );
        assert!(
            cmp.fbip.reuse_fallbacks >= 7,
            "{profile}: shared tree snapshot must fall back, got {}",
            cmp.fbip.reuse_fallbacks
        );
        assert!(
            cmp.fbip.allocations < cmp.baseline.allocations,
            "{profile}: expected fewer allocations (baseline={}, fbip={})",
            cmp.baseline.allocations,
            cmp.fbip.allocations
        );
    }
    sweep_fuel_prefixes("tree_map_bst_insert", source, expected, 500);
}

#[test]
fn constructor_cross_tag_mismatched_layout_and_constrained_gadt_reuse() {
    let source = r#"
module App.Main exposing (main)

type Color = Red Int Int | Black Int Int

type Shape = Single Int | Pair Int Int

type ShowBox =
  | Show a => ShowBox a : ShowBox

flip : Color ->{} Color
let flip c =
  match c with
  | Red a b -> Black (a + 1) (b + 1)
  | Black a b -> Red (a + 1) (b + 1)

collapse : Shape ->{} Shape
let collapse s =
  match s with
  | Single x | Pair x _ -> Single (x + 10)

rebox : ShowBox ->{} ShowBox
let rebox b =
  match b with
  | ShowBox x -> ShowBox (show x ++ "!")

show_box : ShowBox ->{} String
let show_box b =
  match b with
  | ShowBox x -> show x

show_color : Color ->{} String
let show_color c =
  match c with
  | Red a b -> "Red(" ++ show a ++ "," ++ show b ++ ")"
  | Black a b -> "Black(" ++ show a ++ "," ++ show b ++ ")"

show_shape : Shape ->{} String
let show_shape s =
  match s with
  | Single x -> "Single(" ++ show x ++ ")"
  | Pair a b -> "Pair(" ++ show a ++ "," ++ show b ++ ")"

main : Unit ->{Host.Console} Unit
let main () =
  let c1 = flip (flip (Red 1 2))
  let shared_c = Red 10 20
  let flipped_c = flip shared_c
  Platform.stdout_line (show_color c1 ++ ";" ++ show_color shared_c ++ ";" ++ show_color flipped_c)

  let s1 = collapse (Single 5)
  let s2 = collapse (Pair 7 99)
  let shared_s = Pair 3 4
  let s3 = collapse shared_s
  Platform.stdout_line (show_shape s1 ++ ";" ++ show_shape s2 ++ ";" ++ show_shape shared_s ++ ";" ++ show_shape s3)

  let b1 = rebox (ShowBox (42 : Int))
  let shared_b = ShowBox (7 : Int)
  let b2 = rebox shared_b
  Platform.stdout_line (show_box b1 ++ ";" ++ show_box shared_b ++ ";" ++ show_box b2)
"#;
    let expected = "\
Red(3,4);Red(10,20);Black(11,21)
Single(15);Single(17);Pair(3,4);Single(13)
\"42!\";7;\"7!\"
";
    let (dbg, rel) = compare_fbip_vs_baseline("cross_tag_mismatch_gadt", source, expected);
    for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
        assert!(
            cmp.fbip.reuses >= 4,
            "{profile}: expected >= 4 cross-tag/or/GADT reuses, got {}",
            cmp.fbip.reuses
        );
        assert!(
            cmp.fbip.reuse_fallbacks >= 4,
            "{profile}: expected >= 4 fallbacks (layout mismatch + shared aliases), got {}",
            cmp.fbip.reuse_fallbacks
        );
        assert!(
            cmp.fbip.allocations < cmp.baseline.allocations,
            "{profile}: expected fewer allocations (baseline={}, fbip={})",
            cmp.baseline.allocations,
            cmp.fbip.allocations
        );
    }
    sweep_fuel_prefixes("cross_tag_mismatch_gadt", source, expected, 500);
}
