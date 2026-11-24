//! Codegen tests for closed record Go structs carried by value.

use std::collections::BTreeMap;

use super::tests::build;
use super::{Input, go};
use crate::build::builtin_modules;
use crate::build::go::bindings;
use crate::codegen::Known;
use crate::ir::tests::lower_text;
use crate::ir::{Module, lang_items};
use crate::opt;

fn file<'a>(files: &'a [(String, String)], suffix: &str) -> &'a str {
    files
        .iter()
        .find(|(name, _)| name.ends_with(suffix))
        .map(|(_, content)| content.as_str())
        .expect(suffix)
}

#[test]
fn closed_record_uses_by_value_go_struct_direct_field_reads_and_stack_updates() {
    let out = build(
        r"module App exposing (main)

type alias Point = { x : Float; y : Float }

make_record : Float -> Float -> Point
let make_record x y = { x = x; y = y }

make_reversed : Float -> Float -> Point
let make_reversed y x = { y = y; x = x }

read_x : Point -> Float
let read_x p = p.x

bump : Point -> Point
let bump p = { p with x = p.x + 1.0 }

dot : Point -> Float
let dot { x; y } = x * x + y * y

same_point : Point -> Point -> Bool
let same_point a b = a == b

main : Unit ->{Host.Console} Unit
let main () =
  let p = make_record 1.25 2.25
  let q = make_reversed 2.25 1.25
  Platform.stdout_line (show (read_x (bump p), dot q, same_point p q))
",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");
    let records = file(&out.files, "record_types.go");

    // Single Go struct for `{ x : Float; y : Float }` with unboxed float64 fields.
    assert_eq!(records.matches("type R_x_y_").count(), 1, "{records}");
    assert!(
        records.contains("\tf0 float64\n\tf1 float64\n"),
        "{records}"
    );
    assert!(records.contains("func (r R_x_y_"), "{records}");
    assert!(
        records.contains("EqualRecord(other flickrt.Value, strictZero bool) (bool, bool)"),
        "{records}"
    );
    assert!(records.contains("func Eq_R_x_y_"), "{records}");
    assert!(records.contains("func As_R_x_y_"), "{records}");

    // Typed functions take and return the struct by value; `p.x` is direct `.f0`.
    assert!(
        go.contains("func F_App__make_record(fb *flickrt.Fiber, l_x float64, l_y float64) R_x_y_"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__read_x(fb *flickrt.Fiber, l_p R_x_y_")
            && go.contains("return l_p.f0"),
        "{go}"
    );
    // Reversed literal preserves source order (`f1` before `f0`) on the same struct type.
    assert!(go.contains("{f1: l_y, f0: l_x}"), "{go}");
    // Update copies on stack and assigns `.f0` directly.
    assert!(
        go.contains("t1.f0 = flickrt.AddFloat(l_p.f0, float64(1e0))"),
        "{go}"
    );
    // Destructured record parameter stays typed by value.
    assert!(
        go.contains("func F_App__dot(fb *flickrt.Fiber, t") && go.contains("var l_x float64 = t"),
        "{go}"
    );
    // Float record equality uses typed `Eq_R_x_y_...` with no boxing.
    assert!(go.contains("return Eq_R_x_y_"), "{go}");
    assert!(!go.contains("flickrt.Field("), "{go}");
    assert!(!go.contains("flickrt.Shaped("), "{go}");
    assert!(!go.contains("flickrt.With("), "{go}");
}

#[test]
fn closed_record_packs_pointers_first_and_keeps_canonical_indices_and_go_equality() {
    let out = build(
        r#"module App exposing (main)

type alias Packed = { flag : Bool; id : Int; items : List Int; label : String }

make_packed : Bool -> Int -> List Int -> String -> Packed
let make_packed flag id items label = { flag = flag; id = id; items = items; label = label }

type alias Keys = { a : Int; b : String }

eq_keys : Keys -> Keys -> Bool
let eq_keys x y = x == y

main : Unit ->{Host.Console} Unit
let main () =
  let p = make_packed true 7 [1, 2] "hi"
  Platform.stdout_line (show (p.flag, p.id, p.label, eq_keys { a = 1; b = "x" } { b = "x"; a = 1 }))
"#,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");
    let records = file(&out.files, "record_types.go");

    // Canonical label order: f0=flag(bool), f1=id(int32), f2=items(Value), f3=label(string).
    // Struct layout packs pointer-bearing fields first: f2 Value, f3 string, f1 int32, f0 bool.
    assert!(
        records.contains("[]string{\"flag\", \"id\", \"items\", \"label\"}"),
        "{records}"
    );
    assert!(
        records.contains("\tf2 flickrt.Value\n\tf3 string\n\tf1 int32\n\tf0 bool\n"),
        "{records}"
    );
    // All non-float scalar shape (`Keys`) uses native Go `(l_x == l_y)` with no Eq helper.
    assert!(go.contains("return (l_x == l_y)"), "{go}");
    assert!(!records.contains("func Eq_R_a_b_"), "{records}");
}

#[test]
fn closed_record_loops_mut_open_row_and_opaque_alias_boundaries() {
    let out = build(
        r"module App exposing (main)

opaque type alias UserId = Int

type alias Acc = { n : Int; total : Float }

sum_loop : Acc -> Float
let rec sum_loop acc =
  if acc.n <= 0 then acc.total
  else sum_loop { acc with n = acc.n - 1; total = acc.total + 1.5 }

open_n : { n : Int | r } -> Int
let open_n r = r.n

with_alias : { id : UserId } -> Int
let with_alias r = r.id

main : Unit ->{Host.Console} Unit
let main () =
  let mut cur = { n = 3; total = 0.0 }
  cur = { cur with total = 1.0 }
  let a : { id : UserId } = { id = 5 }
  Platform.stdout_line (show (sum_loop cur, open_n cur, with_alias a))
",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    // Self tail loop carries `Acc` by value in a typed loop variable.
    assert!(
        go.contains("func F_App__sum_loop(fb *flickrt.Fiber, t"),
        "{go}"
    );
    assert!(go.contains("var l_acc R_n_total_"), "{go}");
    // `let mut cur` keeps the closed struct Go type.
    assert!(go.contains("var l_cur R_n_total_"), "{go}");
    // Open-row function stays erased (`flickrt.Value`) and reads via `flickrt.Field`.
    assert!(
        go.contains("func F_App__open_n(fb *flickrt.Fiber, l_r flickrt.Value) int32")
            && go.contains("flickrt.Field(l_r, \"n\")"),
        "{go}"
    );
    // Record with opaque-alias field stays erased so it agrees with `{ id : Int }`.
    assert!(
        go.contains("func F_App__with_alias(fb *flickrt.Fiber, l_r flickrt.Value)"),
        "{go}"
    );
}

#[test]
fn release_optimized_ir_preserves_closed_record_structs() {
    let source = r"module App exposing (main, make_record, step)

type alias Point = { x : Float; y : Float }

make_record : Float -> Float -> Point
let make_record x y = { x = x; y = y }

step : Int -> Point -> Point
let rec step n p =
  if n <= 0 then p else step (n - 1) { p with x = p.x + 1.0 }

main : Unit ->{Host.Arguments, Host.Console} Unit
let main () =
  let n = List.length (Platform.args ())
  let p = step n (make_record 1.25 2.25)
  Platform.stdout_line (show (p.x, p.y))
";
    let mut app = lower_text("App", source).expect("lowers");
    let exported = |name: &str| matches!(name, "main" | "make_record" | "step");
    opt::optimize(&mut app, &exported, &|_, _| None, &|_, _| None);
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let items = lang_items();
    let lang = Known::of(&items);
    let foreign = BTreeMap::new();
    let out = go(&Input {
        own: &[&app],
        builtins: &builtins,
        entry: "App",
        test: false,
        suites: &[],
        lang: &lang,
        bindings: bindings(),
        foreign: &foreign,
        paths: &BTreeMap::new(),
        lines: &[],
        sources: &[],
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let code = file(&out.files, "m_App.go");
    let records = file(&out.files, "record_types.go");
    assert!(records.contains("type R_x_y_"), "{records}");
    assert!(code.contains("R_x_y_"), "{code}");
    assert!(!code.contains("flickrt.Shaped("), "{code}");
    assert!(!code.contains("flickrt.Field("), "{code}");
}
