//! Codegen tests for allocation-free concrete `Option` representations on Go.

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

const LAYOUTS_SRC: &str = r#"module App exposing (main)

type Boxed = Boxed Int

opt_int : Int -> Option Int
let opt_int n = if n > 0 then Some n else None

opt_float : Float -> Option Float
let opt_float x = if x > 0.0 then Some x else None

opt_str : String -> Option String
let opt_str s = if s == "" then None else Some s

opt_bool : Bool -> Option Bool
let opt_bool b = if b then Some false else None

opt_adt : Bool -> Boxed -> Option Boxed
let opt_adt ok b = if ok then Some b else None

opt_list : Bool -> List Int -> Option (List Int)
let opt_list ok xs = if ok then Some xs else None

opt_rec : Bool -> { x : Int } -> Option { x : Int }
let opt_rec ok r = if ok then Some r else None

opt_unk : Bool -> Unknown -> Option Unknown
let opt_unk ok u = if ok then Some u else None

opt_nested2 : Int -> Option (Option Int)
let opt_nested2 n =
  if n == 0 then None
  else if n == 1 then Some None
  else Some (Some n)

opt_nested3 : Int -> Option (Option (Option Int))
let opt_nested3 n =
  if n == 0 then None
  else if n == 1 then Some None
  else if n == 2 then Some (Some None)
  else Some (Some (Some n))

opt_nested_adt : Int -> Boxed -> Option (Option Boxed)
let opt_nested_adt n b =
  if n == 0 then None
  else if n == 1 then Some None
  else Some (Some b)

opt_poly : Bool -> a -> Option a
let opt_poly ok x = if ok then Some x else None

unwrap_poly : Option a -> a -> a
let unwrap_poly o fallback =
  match o with
  | Some x -> x
  | None -> fallback

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let b = Boxed 1
  let _ =
    match Json.parse "null" with
    | Ok u -> opt_unk true u
    | Err _ -> None
  let _ = (opt_int 1, opt_float 1.0, opt_str "a", opt_bool true, opt_adt true b, opt_list true [1], opt_rec true { x = 1 })
  let _ = (opt_nested2 2, opt_nested3 3, opt_nested_adt 2 b, unwrap_poly (opt_poly true 9) 0)
  Platform.stdout_line "ok"
"#;

#[test]
fn concrete_option_layouts_choose_sentinel_or_flag_and_preserve_nesting() {
    let out = build(LAYOUTS_SRC);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    // Presence-flag layouts for scalars and Unknown.
    assert!(
        go.contains("func F_App__opt_int(fb *flickrt.Fiber, l_n int32) flickrt.Opt[int32]"),
        "{go}"
    );
    assert!(
        go.contains("flickrt.Opt[int32]{Val: l_n, Ok: true}"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__opt_float(fb *flickrt.Fiber, l_x float64) flickrt.Opt[float64]"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__opt_str(fb *flickrt.Fiber, l_s string) flickrt.Opt[string]"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__opt_unk(fb *flickrt.Fiber, l_ok bool, l_u flickrt.Value) flickrt.Opt[flickrt.Value]"),
        "{go}"
    );

    // Sentinel layouts for Bool and provably non-nil payloads (ADT, List, Record).
    assert!(
        go.contains("func F_App__opt_bool(fb *flickrt.Fiber, l_b bool) flickrt.OptBool"),
        "{go}"
    );
    assert!(go.contains("flickrt.SomeBool(false)"), "{go}");
    assert!(go.contains("flickrt.OptBoolNone"), "{go}");
    assert!(
        go.contains(
            "func F_App__opt_adt(fb *flickrt.Fiber, l_ok bool, l_b flickrt.Value) flickrt.OptRef"
        ),
        "{go}"
    );
    assert!(
        go.contains(
            "func F_App__opt_list(fb *flickrt.Fiber, l_ok bool, l_xs flickrt.Value) flickrt.OptRef"
        ),
        "{go}"
    );
    assert!(
        go.contains(
            "func F_App__opt_rec(fb *flickrt.Fiber, l_ok bool, l_r flickrt.Value) flickrt.OptRef"
        ),
        "{go}"
    );
    assert!(go.contains("flickrt.SomeRef("), "{go}");
    assert!(go.contains("flickrt.NoneRef"), "{go}");

    // Nested Option uses outer presence flag around inner typed Option.
    assert!(
        go.contains(
            "func F_App__opt_nested2(fb *flickrt.Fiber, l_n int32) flickrt.Opt[flickrt.Opt[int32]]"
        ),
        "{go}"
    );
    assert!(
        go.contains("func F_App__opt_nested3(fb *flickrt.Fiber, l_n int32) flickrt.Opt[flickrt.Opt[flickrt.Opt[int32]]]"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__opt_nested_adt(fb *flickrt.Fiber, l_n int32, l_b flickrt.Value) flickrt.Opt[flickrt.OptRef]"),
        "{go}"
    );

    // Polymorphic Option stays erased and uses centralized runtime helpers.
    assert!(
        go.contains(
            "func F_App__opt_poly(fb *flickrt.Fiber, l_ok bool, l_x flickrt.Value) flickrt.Value"
        ),
        "{go}"
    );
    assert!(go.contains("flickrt.IsSome("), "{go}");
    assert!(go.contains("flickrt.UnwrapSome("), "{go}");
}

#[test]
fn typed_option_match_unwrap_equality_and_mut_stay_unboxed() {
    let out = build(
        r"module App exposing (main)

type Tag = Tag Int

match_nested : Option (Option Int) -> Int
let match_nested o =
  match o with
  | Some (Some n) -> n + 1
  | Some None -> -1
  | None -> -2

add_opt : Option Int -> Option Int -> Option Int
let add_opt a b =
  let x = a?
  let y = b?
  Some (x + y)

eq_int_opt : Option Int -> Option Int -> Bool
let eq_int_opt a b = a == b

eq_float_opt : Option Float -> Option Float -> Bool
let eq_float_opt a b = a == b

eq_adt_opt : Option Tag -> Option Tag -> Bool
let eq_adt_opt a b = a == b

eq_nested_opt : Option (Option Int) -> Option (Option Int) -> Bool
let eq_nested_opt a b = a == b

loop_opt : Int -> Option Int -> Int
let rec loop_opt n acc =
  if n <= 0 then
    match acc with
    | Some v -> v
    | None -> 0
  else
    loop_opt (n - 1) (Some n)

mut_opt : Int -> Int
let mut_opt n =
  let mut cur : Option Int = None
  cur = Some n
  match cur with
  | Some v -> v
  | None -> 0

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let a = match_nested (Some (Some 4))
  let b = add_opt (Some 2) (Some 3)
  let c = eq_int_opt b (Some 5)
  let d = eq_float_opt (Some 1.5) (Some 1.5)
  let e = eq_adt_opt (Some (Tag 1)) (Some (Tag 1))
  let f = eq_nested_opt (Some None) None
  let g = loop_opt 3 None
  let h = mut_opt 9
  Platform.stdout_line (show (a, c, d, e, f, g, h))
",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    let fn_body = |name: &str| {
        let start = go
            .find(&format!("func F_App__{name}("))
            .unwrap_or_else(|| panic!("missing F_App__{name}:\n{go}"));
        let rest = &go[start..];
        let end = rest.find("\n\nfunc ").unwrap_or(rest.len());
        &rest[..end]
    };

    for name in [
        "match_nested",
        "add_opt",
        "eq_int_opt",
        "eq_float_opt",
        "eq_adt_opt",
        "eq_nested_opt",
        "loop_opt",
        "mut_opt",
    ] {
        let body = fn_body(name);
        assert!(
            !body.contains("flickrt.NewData") && !body.contains("flickrt.BoxOpt"),
            "typed function `{name}` unexpectedly allocated/boxed:\n{body}"
        );
    }

    assert!(fn_body("match_nested").contains(".Ok"), "{go}");
    assert!(fn_body("match_nested").contains(".Val"), "{go}");
    assert!(
        fn_body("add_opt").contains("if !l_a.Ok {\n\t\treturn flickrt.Opt[int32]{}\n\t}"),
        "{go}"
    );
    assert!(fn_body("eq_int_opt").contains("(l_a == l_b)"), "{go}");
    assert!(fn_body("eq_nested_opt").contains("(l_a == l_b)"), "{go}");
    assert!(
        fn_body("eq_float_opt").contains("flickrt.EqOptFloat(l_a, l_b)"),
        "{go}"
    );
    assert!(
        fn_body("eq_adt_opt").contains("flickrt.EqOptRef(l_a, l_b)"),
        "{go}"
    );
    assert!(
        fn_body("mut_opt").contains("var l_cur flickrt.Opt[int32] = flickrt.Opt[int32]{}"),
        "{go}"
    );
}

#[test]
fn erased_boundaries_convert_typed_options_to_tagged_data() {
    let out = build(
        r"module App exposing (main)

id_poly : a -> a
let id_poly x = x

mk_int : Int -> Option Int
let mk_int n = Some n

mk_nested : Int -> Option (Option Int)
let mk_nested n = Some (Some n)

take_int : Option Int -> Int
let take_int o =
  match o with
  | Some n -> n
  | None -> 0

capture_in_closure : Option Int -> Unit -> Option Int
let capture_in_closure o = fun () -> o

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let a = take_int (id_poly (mk_int 7))
  let b = show (mk_nested 9)
  let f = capture_in_closure (Some 3)
  let c = take_int (f ())
  Platform.stdout_line (show a ++ b ++ show c)
",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    assert!(
        go.contains("flickrt.BoxOptInt(F_App__mk_int(fb, int32(7)))"),
        "{go}"
    );
    assert!(go.contains("flickrt.AsOptInt(F_App__id_poly("), "{go}");
    assert!(
        go.contains("flickrt.BoxOpt(F_App__mk_nested(fb, int32(9)), flickrt.BoxOptInt)"),
        "{go}"
    );
    assert!(go.contains("return flickrt.BoxOptInt(l_o)"), "{go}");
}

#[test]
fn release_optimized_ir_preserves_unboxed_options() {
    let mut ir = lower_text(
        "App",
        r"module App exposing (main, find_opt, use_opt)

find_opt : Int -> Option Int
let rec find_opt n =
  if n <= 0 then None
  else if n == 3 then Some n
  else find_opt (n - 1)

use_opt : Int -> Int
let use_opt n =
  match find_opt n with
  | Some v -> v + 1
  | None -> 0

main : Unit ->{Host.Arguments, Host.Console; AppError} Unit
let main () = Platform.stdout_line (show (use_opt (List.length (Platform.args ()))))
",
    )
    .expect("lowers");
    let exported = |name: &str| matches!(name, "main" | "find_opt" | "use_opt");
    opt::optimize(&mut ir, &exported, &|_, _| None, &|_, _| None);
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let out = go(&Input {
        own: &[&ir],
        builtins: &builtins,
        entry: &ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        foreign: &BTreeMap::new(),
        bindings: bindings(),
        lang: &Known::of(&lang_items()),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go_src = file(&out.files, "m_App.go");
    assert!(
        go_src.contains("func F_App__find_opt(fb *flickrt.Fiber,")
            && go_src.contains(") flickrt.Opt[int32]"),
        "{go_src}"
    );
    assert!(
        !go_src.contains("flickrt.NewData(\"Some\""),
        "release-optimized Go should keep concrete Option unboxed:\n{go_src}"
    );
}

const ALIAS_AND_ERRORS_SRC: &str = r#"module App exposing (main)

opaque type alias UserId = String
opaque type alias Id a = a
opaque type alias MaybeInt = Option Int
opaque type alias Stream = Option (Float, Stream)

type error Boom = Boom String
  deriving Show
type error Bam = Bam Int
  deriving Show

service Tick =
  now : Unit ->{} Int

read_uid : Option UserId -> String
let read_uid u =
  match u with
  | Some s -> s
  | None -> "none"

read_str : Option String -> String
let read_str s =
  match s with
  | Some v -> v
  | None -> "none"

read_id_poly : Option (Id a) -> a -> a
let read_id_poly o fallback =
  match o with
  | Some x -> x
  | None -> fallback

read_id_int : Option (Id Int) -> Int
let read_id_int o =
  match o with
  | Some n -> n
  | None -> 0

read_maybe : Option MaybeInt -> Int
let read_maybe o =
  match o with
  | Some (Some n) -> n
  | Some None -> -1
  | None -> -2

read_stream : Option Stream -> Float
let read_stream o =
  match o with
  | Some (Some (x, _)) -> x
  | _ -> 0.0

read_err : Option (Errors {Boom, Bam}) -> Bool
let read_err o =
  match o with
  | Some _ -> true
  | None -> false

read_err_single : Option (Errors {Boom}) -> Bool
let read_err_single o =
  match o with
  | Some _ -> true
  | None -> false

read_svc : ServiceTag r -> Option (ServiceImpl r) -> Bool
let read_svc _ o =
  match o with
  | Some _ -> true
  | None -> false

read_svc_concrete : Option Tick.Impl -> Bool
let read_svc_concrete o =
  match o with
  | Some _ -> true
  | None -> false

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let s : Option String = Some "alice"
  let uid : Option UserId = Some "bob"
  let oi : Option Int = Some 7
  let idi : Option (Id Int) = Some 8
  let mi : Option (Option Int) = Some (Some 9)
  let st : Option Stream = Some (Some (1.5, None))
  let be : Option Boom = Some (Boom "oops")
  let errs : Option (Errors {Boom, Bam}) = None
  let sv : Option { now : Unit ->{} Int } = Some { now = fun () -> 1 }
  let _ = (read_uid s, read_str uid, read_id_poly idi 0, read_id_int oi, read_maybe mi, read_stream st)
  let _ = (read_err errs, read_err_single be, read_svc Tick sv, read_svc_concrete sv)
  Platform.stdout_line "ok"
"#;

#[test]
fn opaque_alias_errors_and_service_impl_options_stay_erased() {
    let out = build(ALIAS_AND_ERRORS_SRC);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    // Opaque aliases, Errors, and ServiceImpl stay erased `flickrt.Value`.
    for name in [
        "read_uid",
        "read_id_poly",
        "read_id_int",
        "read_maybe",
        "read_stream",
        "read_err",
        "read_svc",
    ] {
        let sig = format!("func F_App__{name}(fb *flickrt.Fiber, ");
        let start = go
            .find(&sig)
            .unwrap_or_else(|| panic!("missing {sig}\n{go}"));
        let line = &go[start..go[start..].find('\n').unwrap_or(go.len() - start) + start];
        assert!(
            line.contains("flickrt.Value)"),
            "expected erased Option parameter for `{name}`, got: {line}"
        );
    }
}

#[test]
fn nested_or_pattern_boxes_typed_option_when_outer_or_widens_to_value() {
    let out = build(
        r#"module App exposing (main)

type Node = Wrap (Option Node)

direct_nested_or : Option Node -> Option Node
let direct_nested_or o =
  let (Some (Wrap x) | (x | x)) = o
  x

deep_nested_or : Option Node -> Option Node
let deep_nested_or o =
  let ((Some (Wrap x) | (x | x)) | x) = o
  x

match_nested_or : Option (Option Node) -> Option Node
let match_nested_or o =
  match o with
  | Some (Some (Wrap x)) | Some (x | x) -> x
  | None -> None

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let _ = direct_nested_or None
  let _ = deep_nested_or None
  let _ = match_nested_or (Some None)
  Platform.stdout_line "ok"
"#,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    let fn_body = |name: &str| {
        let start = go
            .find(&format!("func F_App__{name}("))
            .unwrap_or_else(|| panic!("missing F_App__{name}:\n{go}"));
        let rest = &go[start..];
        let end = rest.find("\n\nfunc ").unwrap_or(rest.len());
        &rest[..end]
    };

    for name in ["direct_nested_or", "deep_nested_or"] {
        let body = fn_body(name);
        assert!(body.contains("var l_x flickrt.Value"), "{body}");
        assert!(body.contains("l_x = flickrt.BoxOptRef(l_o)"), "{body}");
        assert!(
            !body.contains("l_x = l_o;"),
            "inner Pat::Or leaked unboxed OptRef into flickrt.Value in `{name}`:\n{body}"
        );
    }

    let matched = fn_body("match_nested_or");
    assert!(matched.contains("var l_x flickrt.Value"), "{matched}");
    assert!(
        matched.contains("l_x = flickrt.BoxOptRef(l_o.Val)"),
        "{matched}"
    );
}

#[test]
fn record_field_option_unifies_with_record_option_as_opt_ref() {
    let out = build(
        r#"module App exposing (main)

read_field : Option (Record.Field "x" Int) -> Int
let read_field o =
  match o with
  | Some r -> r.x
  | None -> 0

read_rec : Option { x : Int } -> Int
let read_rec o =
  match o with
  | Some r -> r.x
  | None -> 0

make_field : Int -> Option (Record.Field "x" Int)
let make_field n = Some { x = n }

read_field_poly : Record.HasField l (Record.Field l a) a => Record.Label l -> Option (Record.Field l a) -> a -> a
let read_field_poly lbl o fallback =
  match o with
  | Some r -> Record.get lbl r
  | None -> fallback

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let r : Option { x : Int } = Some { x = 10 }
  let f : Option (Record.Field "x" Int) = Some { x = 20 }
  let from_fn : Option { x : Int } = make_field 30
  let a = read_field r
  let b = read_rec f
  let c = read_rec from_fn
  let d = read_field_poly (Record.label : Record.Label "x") r 0
  let e = read_field_poly (Record.label : Record.Label "x") f 0
  Platform.stdout_line (show (a, b, c, d, e))
"#,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    assert!(
        go.contains("func F_App__read_field(fb *flickrt.Fiber, l_o flickrt.OptRef) int32"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__read_rec(fb *flickrt.Fiber, l_o flickrt.OptRef) int32"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__make_field(fb *flickrt.Fiber, l_n int32) flickrt.OptRef"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__read_field_poly(") && go.contains("l_o flickrt.OptRef"),
        "{go}"
    );
}
