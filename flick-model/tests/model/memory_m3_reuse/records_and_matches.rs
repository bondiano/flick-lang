//! Record update/rebuild reuse and partial/guarded/wildcard/nested match reuse tests.

use super::{compare_fbip_vs_baseline, sweep_fuel_prefixes};

const RECORD_REUSE_SOURCE: &str = r#"
module App.Main exposing (main)

import Arena

type alias Point = { x : Int; y : Int }
type alias Point3 = { x : Int; y : Int; z : Int }

step_update : Int -> Point ->{} Point
let rec step_update n p =
  if n <= 0 then p
  else step_update (n - 1) { p with x = p.x + 1; y = p.y + 2 }

step_rebuild_pat : Int -> Point ->{} Point
let rec step_rebuild_pat n p =
  if n <= 0 then p
  else
    let { x; y } = p
    step_rebuild_pat (n - 1) { x = x + 1; y = y + 3 }

step_rebuild_match : Int -> Point ->{} Point
let rec step_rebuild_match n p =
  if n <= 0 then p
  else
    match p with
    | { x; y } -> step_rebuild_match (n - 1) { x = x + 2; y = y + 1 }

drop_z : Point3 ->{} Point
let drop_z p =
  let { x; y; z } = p
  { x = x + z; y = y }

main : Unit ->{Host.Console} Unit
let main () =
  let p1 = step_update 15 { x = 0; y = 0 }
  Platform.stdout_line ("upd=" ++ show p1.x ++ "," ++ show p1.y)

  let p2 = step_rebuild_pat 15 { x = 0; y = 0 }
  Platform.stdout_line ("pat=" ++ show p2.x ++ "," ++ show p2.y)

  let p3 = step_rebuild_match 15 { x = 0; y = 0 }
  Platform.stdout_line ("mat=" ++ show p3.x ++ "," ++ show p3.y)

  let orig = { x = 10; y = 20 }
  let updated_shared = { orig with x = 99 }
  let rebuilt_shared =
    match orig with
    | { x; y } -> { x = x + 5; y = y + 5 }
  let rcell = Ref.make { x = 1; y = 2 }
  let from_ref = Ref.get rcell
  let mod_ref = { from_ref with x = 77 }
  let from_ref_after = Ref.get rcell
  let arena_out =
    scope {
      let a = Arena.make ()
      let v = Arena.alloc a { x = 3; y = 4 }
      let got = Arena.get v
      let mod_a = { got with y = 88 }
      let got_after = Arena.get v
      show got_after.x ++ "," ++ show got_after.y ++ "/" ++ show mod_a.x ++ "," ++ show mod_a.y
    }
  Platform.stdout_line ("orig=" ++ show orig.x ++ "," ++ show orig.y)
  Platform.stdout_line ("upd_sh=" ++ show updated_shared.x ++ "," ++ show updated_shared.y)
  Platform.stdout_line ("reb_sh=" ++ show rebuilt_shared.x ++ "," ++ show rebuilt_shared.y)
  Platform.stdout_line ("ref=" ++ show from_ref_after.x ++ "," ++ show from_ref_after.y ++ "/" ++ show mod_ref.x ++ "," ++ show mod_ref.y)
  Platform.stdout_line ("arena=" ++ arena_out)

  let p2d = drop_z { x = 4; y = 5; z = 6 }
  Platform.stdout_line ("dropz=" ++ show p2d.x ++ "," ++ show p2d.y)
"#;

#[test]
fn record_update_and_record_rebuild_reuse_with_shared_ref_arena_and_width_fallbacks() {
    let expected = "\
upd=15,30
pat=15,45
mat=30,15
orig=10,20
upd_sh=99,20
reb_sh=15,25
ref=1,2/77,2
arena=3,4/3,88
dropz=10,5
";
    let (dbg, rel) =
        compare_fbip_vs_baseline("records_update_and_rebuild", RECORD_REUSE_SOURCE, expected);
    for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
        assert!(
            cmp.fbip.reuses >= 45,
            "{profile}: expected >= 45 record reuses (Update + ReuseRecord), got {}",
            cmp.fbip.reuses
        );
        let min_fallbacks = if profile == "debug" { 5 } else { 4 };
        assert!(
            cmp.fbip.reuse_fallbacks >= min_fallbacks,
            "{profile}: expected >= {min_fallbacks} fallbacks (shared + Ref + Arena + width mismatch), got {}",
            cmp.fbip.reuse_fallbacks
        );
        assert!(
            cmp.fbip.allocations + 45 <= cmp.baseline.allocations,
            "{profile}: expected >= 45 fewer allocations (baseline={}, fbip={})",
            cmp.baseline.allocations,
            cmp.fbip.allocations
        );
    }
    sweep_fuel_prefixes(
        "records_update_and_rebuild",
        RECORD_REUSE_SOURCE,
        expected,
        600,
    );
}

#[test]
fn partial_matches_guards_wildcards_nested_patterns_and_whole_alias_safety() {
    let source = r#"
module App.Main exposing (main)

type Boxed = Box Int
type PairBox = PairBox Boxed Boxed
type Item = Keep Int | Drop Int | Tag Int

classify : Item ->{} Item
let classify item =
  match item with
  | Keep x -> Keep (x * 2)
  | Drop _ -> Tag 0
  | Tag x when x > 10 -> Keep x
  | _ -> Tag 99

bump_pair : PairBox ->{} PairBox
let bump_pair p =
  match p with
  | PairBox (Box a) (Box b) -> PairBox (Box (a + 1)) (Box (b + 2))

duplicate_with_alias : Boxed ->{} (Boxed, Boxed)
let duplicate_with_alias b =
  match b with
  | whole ->
      match whole with
      | Box n -> (whole, Box (n + 100))

show_item : Item ->{} String
let show_item it =
  match it with
  | Keep x -> "Keep(" ++ show x ++ ")"
  | Drop x -> "Drop(" ++ show x ++ ")"
  | Tag x -> "Tag(" ++ show x ++ ")"

main : Unit ->{Host.Console} Unit
let main () =
  let a = classify (Keep 5)
  let b = classify (Drop 7)
  let c = classify (Tag 20)
  let d = classify (Tag 3)
  let sh_item = Tag 25
  let e = classify sh_item
  Platform.stdout_line (show_item a ++ ";" ++ show_item b ++ ";" ++ show_item c ++ ";" ++ show_item d ++ ";" ++ show_item sh_item ++ ";" ++ show_item e)

  let p = bump_pair (PairBox (Box 10) (Box 20))
  match p with
  | PairBox (Box x) (Box y) -> Platform.stdout_line ("pair=" ++ show x ++ "," ++ show y)

  let (orig_b, bumped_b) = duplicate_with_alias (Box 7)
  match (orig_b, bumped_b) with
  | (Box o, Box u) -> Platform.stdout_line ("alias=" ++ show o ++ "," ++ show u)
"#;
    let expected = "\
Keep(10);Tag(0);Keep(20);Tag(99);Tag(25);Keep(25)
pair=11,22
alias=7,107
";
    let (dbg, rel) = compare_fbip_vs_baseline("partial_matches_guards_nested", source, expected);
    for (profile, cmp) in [("debug", &dbg), ("release", &rel)] {
        assert!(
            cmp.fbip.reuses >= 6,
            "{profile}: expected >= 6 partial/guard/wildcard/nested reuses, got {}",
            cmp.fbip.reuses
        );
        assert!(
            cmp.fbip.allocations < cmp.baseline.allocations,
            "{profile}: expected fewer allocations (baseline={}, fbip={})",
            cmp.baseline.allocations,
            cmp.fbip.allocations
        );
    }
    sweep_fuel_prefixes("partial_matches_guards_nested", source, expected, 500);
}
