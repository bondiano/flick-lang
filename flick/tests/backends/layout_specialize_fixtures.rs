//! Flick source fixtures for `layout_specialize` e2e backend tests (#739, #743).

pub(super) const SPECIALIZED_AND_LAYOUTS_SOURCE: &str = r#"
module Fixture.LayoutSpecialize exposing (main, opt_map, fold_sum)

type Shape =
  | Circle Float
  | Rect Float Float

type alias Metrics = { flag : Bool; value : Float; tag : Int }

opt_map : (a -> b) -> Option a -> Option b
let opt_map f opt =
  match opt with
  | None -> None
  | Some x -> Some (f x)

fold_sum : (a -> a -> a) -> a -> List a -> a
let rec fold_sum add acc xs =
  match xs with
  | [] -> acc
  | [h, ...t] -> fold_sum add (add acc h) t

area : Shape -> Float
let area s =
  match s with
  | Circle r -> r * r
  | Rect w h -> w * h

main : Unit ->{Host.Console} Unit
let main () =
  let c = Circle 3.0
  let r = Rect 2.0 5.0
  let shape_total = area c + area r
  let mapped = opt_map (fun x -> x + 1.5) (Some shape_total)
  let opt_val =
    match mapped with
    | Some v -> v
    | None -> 0.0
  let list_total = fold_sum (fun a b -> a + b) 0 [10, 20, 12]
  let rec_val : Metrics = { flag = true; value = opt_val; tag = list_total }
  let flag_str = if rec_val.flag then "T" else "F"
  Platform.stdout_line
    ("shape=" ++ show shape_total
      ++ "|opt=" ++ show opt_val
      ++ "|sum=" ++ show list_total
      ++ "|rec=" ++ flag_str ++ ":" ++ show rec_val.value ++ ":" ++ show rec_val.tag)
"#;

pub(super) const FALLBACK_FORMS_SOURCE: &str = r##"
module Fixture.LayoutSpecialize exposing (main, poly_bounce, apply_hr, render_box, use_open_row)

// 1. Polymorphic recursion (compiles once over boxed cells)
poly_bounce : forall a. Int -> a -> a
let rec poly_bounce n x =
  if n <= 0 then x
  else
    let pair = poly_bounce (n - 1) (x, x)
    match pair with
    | (left, _) -> left

// 2. Higher-rank callback
apply_hr : (forall a. a -> a) -> (Int, String) -> (Int, String)
let apply_hr f pair =
  match pair with
  | (n, s) -> (f n, f s)

// 3. Packed modules with two distinct local dictionaries at the same concrete type (Int)
//    matching `tests/run/codegen/unpack_dictionary.fl` precedence rules.
type module Shown =
  type Item = Int
  class Show = Prelude.Show
  showing : Show.Dict Item
  instance Show Item from showing

module Hashed exposing (Item, Show(..), showing) =
  import Prelude exposing (Show(..))
  type alias Item = Int
  showing : Show.Dict Item
  let showing = Show.dictionary { show = fun n -> "#" ++ show n }

module Bracketed exposing (Item, Show(..), showing) =
  import Prelude exposing (Show(..))
  type alias Item = Int
  showing : Show.Dict Item
  let showing = Show.dictionary { show = fun n -> "[" ++ show n ++ "]" }

packed_hashed : Module Shown
let packed_hashed = pack Hashed as Shown

packed_bracketed : Module Shown
let packed_bracketed = pack Bracketed as Shown

shown : Show a => a -> String
let shown x = "<" ++ show x ++ ">"

// 4. GADT with existential + held class dictionary
type ShowBox =
  | Show b => ShowBox b : ShowBox

render_box : ShowBox -> String
let render_box box =
  match box with
  | ShowBox x -> "box:" ++ show x

// 5. Open-row record helper (also exercised through unspecialized open-row wrapper)
read_x : { x : Int | r } -> Int
let read_x row = row.x

use_open_row : (forall r. { x : Int | r } -> Int) -> Int
let use_open_row getter =
  getter { x = 11; y = "a" } + getter { x = 31; flag = true }

main : Unit ->{Host.Console} Unit
let main () =
  let pr_i = poly_bounce 2 7
  let pr_s = poly_bounce 2 "ok"
  let (hr_i, hr_s) = apply_hr (fun x -> x) (pr_i, pr_s)
  let local_hash =
    unpack packed_hashed as K in show 5 ++ " " ++ shown 6 ++ " " ++ show (Some 5)
  let local_brack =
    unpack packed_bracketed as K in show 5 ++ " " ++ shown 6 ++ " " ++ show (Some 5)
  let global_show = show 5 ++ " " ++ shown 6 ++ " " ++ show (Some 5)
  let g1 = render_box (ShowBox 99)
  let g2 = render_box (ShowBox "hi")
  let row_sum = use_open_row read_x
  // 6. Polymorphic foreign (`List.length` -> `List.Core.length`)
  let flen = List.length [10, 20, 30] + List.length ["a", "b"]
  Platform.stdout_line
    ("pr=" ++ show hr_i ++ ":" ++ hr_s
      ++ "|hash=" ++ local_hash
      ++ "|brack=" ++ local_brack
      ++ "|glob=" ++ global_show
      ++ "|gadt=" ++ g1 ++ "," ++ g2
      ++ "|row=" ++ show row_sum
      ++ "|flen=" ++ show flen)
"##;

pub(super) const BUDGET_EXCEEDED_SOURCE: &str = r"
module Fixture.LayoutSpecialize exposing (main)

f0 : a -> Int
let f0 _ = 1

f1 : a -> Int
let f1 x = f0 (x, 1) + f0 (x, true)

f2 : a -> Int
let f2 x = f1 (x, 1) + f1 (x, true)

f3 : a -> Int
let f3 x = f2 (x, 1) + f2 (x, true)

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (f3 0))
";
