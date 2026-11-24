//! Codegen tests for by-value closed ADTs and concrete `Result` instantiations on Go.

use std::collections::BTreeMap;

use super::tests::build;
use super::{Input, go};
use crate::build::builtin_modules;
use crate::build::go::bindings;
use crate::codegen::Known;
use crate::ir::tests::{check_text_with, lower_text};
use crate::ir::{Module, lang_items};
use crate::opt;
use crate::types;

fn file<'a>(files: &'a [(String, String)], suffix: &str) -> &'a str {
    files
        .iter()
        .find(|(name, _)| name.ends_with(suffix))
        .map(|(_, content)| content.as_str())
        .expect(suffix)
}

fn fn_body<'a>(go_src: &'a str, full_name: &str) -> &'a str {
    let start = go_src
        .find(&format!("func {full_name}("))
        .unwrap_or_else(|| panic!("missing {full_name}:\n{go_src}"));
    let rest = &go_src[start..];
    let end = rest.find("\n\nfunc ").unwrap_or(rest.len());
    &rest[..end]
}

const MULTI_TYPES_SRC: &str = r"module Types exposing (Status(..), Shape(..), Job, Envelope(..))

type Status = Pending | Active Int | Failed String
  deriving Eq, Ord, Hash, Show

type Shape = Dot | Circle Float | Rect Float Float
  deriving Eq, Ord, Hash, Show

type alias Job = { id : Int; status : Status; outcome : Result String Int }

type Envelope = Direct Status | Checked (Result String Int)
  deriving Eq, Ord, Hash, Show
";

const MULTI_WORKER_SRC: &str = r#"module Worker exposing (make_status, make_outcome, make_job, score_job, wrap_env, read_env, same_status, same_shape, loop_status)

import Types exposing (Status(..), Shape(..), Job, Envelope(..))

make_status : Int -> Status
let make_status n =
  if n == 0 then Pending
  else if n > 0 then Active n
  else Failed "neg"

make_outcome : Int -> Result String Int
let make_outcome n =
  if n >= 0 then Ok (n * 2) else Err "bad"

make_job : Int -> Job
let make_job n = { id = n; status = make_status n; outcome = make_outcome n }

score_job : Job -> Int
let score_job job =
  let a =
    match job.status with
    | Pending -> 0
    | Active v -> v
    | Failed _ -> -1
  let b =
    match job.outcome with
    | Ok v -> v
    | Err _ -> -2
  job.id + a + b

wrap_env : Int -> Envelope
let wrap_env n = if n > 0 then Direct (Active n) else Checked (Err "zero")

read_env : Envelope -> Int
let read_env env =
  match env with
  | Direct (Active v) -> v
  | Direct _ -> 0
  | Checked (Ok v) -> v
  | Checked (Err _) -> -1

same_status : Status -> Status -> Bool
let same_status a b = a == b

same_shape : Shape -> Shape -> Bool
let same_shape a b = a == b

loop_status : Int -> Status -> Int
let rec loop_status n acc =
  if n <= 0 then
    match acc with
    | Pending -> 0
    | Active v -> v
    | Failed _ -> -1
  else
    loop_status (n - 1) (Active (n + 1))
"#;

const MULTI_APP_SRC: &str = r"module App exposing (main)

import Types exposing (Status(..), Shape(..))
import Worker

main : Unit ->{Host.Console} Unit
let main () =
  let mut cur : Status = Pending
  cur = Worker.make_status 3
  let j = Worker.make_job 4
  let e = Worker.wrap_env 5
  let s1 = Worker.same_status cur (Active 3)
  let s2 = Worker.same_shape (Circle 2.0) (Circle 2.0)
  let lp = Worker.loop_status 3 Pending
  Platform.stdout_line (show (Worker.score_job j, Worker.read_env e, s1, s2, lp))
";

#[test]
fn closed_adt_and_concrete_result_stay_by_value_across_calls_aggregates_and_modules() {
    let mut ifaces = types::builtins().to_vec();
    let (types_ir, types_iface) =
        check_text_with("Types", MULTI_TYPES_SRC, &ifaces).expect("Types");
    ifaces.push(types_iface);
    let (worker_ir, worker_iface) =
        check_text_with("Worker", MULTI_WORKER_SRC, &ifaces).expect("Worker");
    ifaces.push(worker_iface);
    let (app_ir, _) = check_text_with("App", MULTI_APP_SRC, &ifaces).expect("App");

    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let out = go(&Input {
        own: &[&types_ir, &worker_ir, &app_ir],
        builtins: &builtins,
        entry: &app_ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: bindings(),
        lang: &Known::of(&lang_items()),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let worker_go = file(&out.files, "m_Worker.go");
    let app_go = file(&out.files, "m_App.go");
    let adts_go = file(&out.files, "adt_types.go");
    let records_go = file(&out.files, "record_types.go");

    // Dedicated Go value structs declared once in adt_types.go.
    assert!(
        adts_go.contains("type A_Types__Status struct {\n\tf0 string\n\tf1 int32\n\ttag uint8\n}"),
        "{adts_go}"
    );
    assert!(
        adts_go
            .contains("type A_Types__Shape struct {\n\tf0 float64\n\tf1 float64\n\ttag uint8\n}"),
        "{adts_go}"
    );
    assert!(adts_go.contains("type AI_Prelude__Result__"), "{adts_go}");
    assert!(
        adts_go.contains("type A_Types__Envelope struct"),
        "{adts_go}"
    );
    // Closed record `Job` embeds `A_Types__Status` and `AI_Prelude__Result__*` by value.
    assert!(
        records_go.contains("A_Types__Status") && records_go.contains("AI_Prelude__Result__"),
        "{records_go}"
    );

    // Every typed Worker function has zero `flickrt.Value`, zero `any`, zero `Box_A_`, zero heap ctors.
    for name in [
        "F_Worker__make_status",
        "F_Worker__make_outcome",
        "F_Worker__make_job",
        "F_Worker__score_job",
        "F_Worker__wrap_env",
        "F_Worker__read_env",
        "F_Worker__same_status",
        "F_Worker__same_shape",
        "F_Worker__loop_status",
    ] {
        let body = fn_body(worker_go, name);
        assert!(
            !body.contains("flickrt.Value")
                && !body.contains("Box_A")
                && !body.contains("New_T_")
                && !body.contains("flickrt.NewData")
                && !body.contains("flickrt.Ok(")
                && !body.contains("flickrt.Err("),
            "typed function `{name}` unexpectedly boxed/allocated:\n{body}"
        );
    }

    // Matching checks `.tag` and reads typed fields directly; comparable ADTs use `==`, float ADTs use `Eq_A_`.
    assert!(
        fn_body(worker_go, "F_Worker__score_job").contains(".tag =="),
        "{worker_go}"
    );
    assert!(
        fn_body(worker_go, "F_Worker__same_status").contains("(l_a == l_b)"),
        "{worker_go}"
    );
    assert!(
        fn_body(worker_go, "F_Worker__same_shape").contains("Eq_A_Types__Shape(l_a, l_b)"),
        "{worker_go}"
    );
    // Mutable local stays typed across modules.
    assert!(
        app_go.contains("var l_cur A_Types__Status = A_Types__Status{}"),
        "{app_go}"
    );
}

#[test]
fn closed_adt_derived_ops_stay_typed_and_custom_instances_are_not_bypassed() {
    let out = build(
        r#"module App exposing (main)

type Shape = Dot | Circle Float | Rect Float Float
  deriving Eq, Ord, Hash, Show

type Custom = CVal Int

instance Eq Custom =
  let eq _ _ = true

instance Show Custom =
  let show _ = "custom-shown"

derived_ops : Shape -> Shape -> (Bool, Bool, Ordering, Int, String)
let derived_ops a b = (eq a b, a < b, compare a b, hash a, show b)

result_ops : Result String Int -> Result String Int -> (Bool, Bool, Ordering, Int, String)
let result_ops a b = (eq a b, a <= b, compare a b, hash a, show b)

custom_ops : Custom -> Custom -> (Bool, String)
let custom_ops a b = (eq a b, show a)

main : Unit ->{Host.Console} Unit
let main () =
  let d = derived_ops (Circle 1.5) (Rect 2.0 3.0)
  let r = result_ops (Ok 10) (Err "e")
  let c = custom_ops (CVal 1) (CVal 2)
  Platform.stdout_line (show (d, r, c))
"#,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go_src = file(&out.files, "m_App.go");
    let adts = file(&out.files, "adt_types.go");

    // Derived ops on Shape and concrete Result stay completely unboxed via adt_types.go helpers.
    for name in ["F_App__derived_ops", "F_App__result_ops"] {
        let body = fn_body(go_src, name);
        assert!(
            !body.contains("Box_A")
                && !body.contains("F_App__Eq_")
                && !body.contains("F_App__Ord_")
                && !body.contains("F_App__Hash_")
                && !body.contains("F_App__Show_")
                && !body.contains("F_Instances__"),
            "`{name}` unexpectedly went through erased instance method:\n{body}"
        );
    }
    assert!(
        fn_body(go_src, "F_App__derived_ops").contains("Eq_A_App__Shape(l_a, l_b)"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__derived_ops").contains("(Cmp_A_App__Shape(l_a, l_b) < 0)"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__derived_ops").contains("Ord_A_App__Shape(l_a, l_b)"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__derived_ops").contains("Hash_A_App__Shape(l_a)"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__derived_ops").contains("Show_A_App__Shape(l_b)"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__result_ops").contains("(l_a == l_b)"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__result_ops").contains("(Cmp_AI_Prelude__Result__"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__result_ops").contains("Hash_AI_Prelude__Result__"),
        "{go_src}"
    );
    assert!(
        fn_body(go_src, "F_App__result_ops").contains("Show_AI_Prelude__Result__"),
        "{go_src}"
    );

    // Result Hash matches Instances.fl (`Ok a -> hash a`, `Err e -> 1 + hash e`).
    assert!(
        adts.contains("return int32(1) + flickrt.HashString(v.f0)"),
        "{adts}"
    );

    // Custom hand-written Eq and Show instances are NOT bypassed.
    let custom = fn_body(go_src, "F_App__custom_ops");
    assert!(custom.contains("F_App__Eq_x20_Custom_x24_eq("), "{custom}");
    assert!(
        custom.contains("F_App__Show_x20_Custom_x24_show("),
        "{custom}"
    );
    assert!(!custom.contains("Show_A_App__Custom"), "{custom}");
}

#[test]
fn closed_adt_erased_boundaries_and_release_optimization() {
    let src = r#"module App exposing (main, make_shape, area, make_res, read_res)

type Shape = Dot | Circle Float | Rect Float Float

id_poly : a -> a
let id_poly x = x

make_shape : Float -> Shape
let make_shape r = if r > 0.0 then Circle r else Dot

area : Shape -> Float
let area s =
  match s with
  | Dot -> 0.0
  | Circle r -> r * r
  | Rect w h -> w * h

make_res : Int -> Result String Int
let make_res n = if n > 0 then Ok n else Err "no"

read_res : Result String Int -> Int
let read_res r =
  match r with
  | Ok v -> v
  | Err _ -> -1

main : Unit ->{Host.Arguments, Host.Console} Unit
let main () =
  let n = List.length (Platform.args ())
  let a = area (id_poly (make_shape (Int.to_float n)))
  let b = read_res (id_poly (make_res n))
  let c = read_res (Result.map (fun x -> x + 1) (make_res (n + 1)))
  Platform.stdout_line (show (a, b, c))
"#;
    let out = build(src);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let debug_go = file(&out.files, "m_App.go");
    // Erased polymorphic boundaries box/unbox cleanly via Box_A_* / As_A_*.
    assert!(debug_go.contains("Box_A_App__Shape("), "{debug_go}");
    assert!(debug_go.contains("As_A_App__Shape("), "{debug_go}");
    assert!(debug_go.contains("Box_AI_Prelude__Result__"), "{debug_go}");
    assert!(debug_go.contains("As_AI_Prelude__Result__"), "{debug_go}");

    // Release-optimized IR preserves by-value closed ADTs and concrete Result.
    let mut ir = lower_text("App", src).expect("lowers");
    let exported = |name: &str| {
        matches!(
            name,
            "main" | "make_shape" | "area" | "make_res" | "read_res"
        )
    };
    opt::optimize(&mut ir, &exported, &|_, _| None, &|_, _| None);
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let rel_out = go(&Input {
        own: &[&ir],
        builtins: &builtins,
        entry: &ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: bindings(),
        lang: &Known::of(&lang_items()),
    });
    assert!(rel_out.diagnostics.is_empty(), "{:?}", rel_out.diagnostics);
    let rel_go = file(&rel_out.files, "m_App.go");
    assert!(rel_go.contains("A_App__Shape{tag: 1,"), "{rel_go}");
    assert!(
        rel_go.contains("func F_App__area(fb *flickrt.Fiber, l_s A_App__Shape) float64"),
        "{rel_go}"
    );
    assert!(
        rel_go.contains("func F_App__make_res(fb *flickrt.Fiber, l_n int32) AI_Prelude__Result__"),
        "{rel_go}"
    );
    assert!(
        rel_go.contains("func F_App__read_res(fb *flickrt.Fiber, l_r AI_Prelude__Result__"),
        "{rel_go}"
    );
}

#[test]
fn all_nullary_adt_unbox_omits_unused_switch_variable() {
    let out = build(
        "module App exposing (main)\n\
         type Color = Red | Green | Blue\n\
         id_poly : a -> a\nlet id_poly x = x\n\
         is_red : Color -> Bool\nlet is_red c = c == Red\n\
         main : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (is_red (id_poly Green)))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let adts = file(&out.files, "adt_types.go");
    assert!(adts.contains("\tswitch v.(type) {"), "{adts}");
    assert!(!adts.contains("\tswitch x := v.(type) {"), "{adts}");
}
