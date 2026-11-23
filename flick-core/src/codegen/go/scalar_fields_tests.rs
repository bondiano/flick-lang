//! The Go of unboxed scalar fields in program-owned constructors, their
//! pattern bindings, and zero-reboxing derived Eq/Ord/Hash/Show methods.

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
fn scalar_fields_pack_pointers_first_and_expose_runtime_boundaries() {
    let out = build(
        "module App exposing (main)\n\n\
         type Entry = Entry Int Float Bool String | Mixed Bool (Option Int) Int String\n\n\
         read_entry : Entry -> String\n\
         let read_entry e =\n  match e with\n  | Entry i f b s -> show i ++ show f ++ show b ++ s\n  \
         | Mixed b _ i s -> show b ++ show i ++ s\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (read_entry (Entry 1 2.5 true \"ok\") ++ read_entry (Mixed false (Some 3) 4 \"hi\"))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let types = file(&out.files, "own_types.go");

    // Pointer-bearing fields (interface, string) precede non-pointer scalars (float64, int32, bool).
    assert!(
        types.contains(
            "type T_App__Entry struct {\n\tf3 string\n\tf1 float64\n\tf0 int32\n\tf2 bool\n}"
        ),
        "{types}"
    );
    assert!(
        types.contains(
            "type T_App__Mixed struct {\n\tf1 flickrt.Value\n\tf3 string\n\tf2 int32\n\tf0 bool\n}"
        ),
        "{types}"
    );
    // Constructor takes declaration-order typed arguments and fills by field name.
    assert!(
        types.contains("func New_T_App__Entry(f0 int32, f1 float64, f2 bool, f3 string) flickrt.Value {\n\treturn &T_App__Entry{f0: f0, f1: f1, f2: f2, f3: f3}\n}"),
        "{types}"
    );
    // Runtime boundary adapters preserve declaration order and equality semantics.
    assert!(
        types.contains(
            "func (p *T_App__Entry) Fields() []flickrt.Value { return []flickrt.Value{p.f0, p.f1, p.f2, p.f3} }"
        ),
        "{types}"
    );
    assert!(
        types.contains("p.f0 == q.f0 && flickrt.EqualFloat(p.f1, q.f1, strictZero) && p.f2 == q.f2 && p.f3 == q.f3"),
        "{types}"
    );
    assert!(
        types.contains("flickrt.EqualWith(p.f1, q.f1, strictZero)"),
        "{types}"
    );
    assert!(
        types.contains(
            "&T_App__Entry{f0: flickrt.AsInt(f[0]), f1: flickrt.AsFloat(f[1]), f2: flickrt.AsBool(f[2]), f3: flickrt.AsString(f[3])}"
        ),
        "{types}"
    );
}

#[test]
fn polymorphic_and_mixed_constructors_unbox_only_declaration_site_scalars() {
    let out = build(
        "module App exposing (main)\n\n\
         type Box a = Box a\n\
         type Tagged a = Tagged Int a\n\n\
         unbox_int : Box Int -> Int\n\
         let unbox_int b =\n  match b with\n  | Box x -> x\n\n\
         tag_of : Tagged String -> Int\n\
         let tag_of t =\n  match t with\n  | Tagged n _ -> n\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (unbox_int (Box 7), tag_of (Tagged 9 \"a\")))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let types = file(&out.files, "own_types.go");
    let go = file(&out.files, "m_App.go");

    // `Box a` is polymorphic at its declaration site, so it stays boxed even when used at `Int`.
    assert!(
        types.contains("type T_App__Box struct{ f [1]flickrt.Value }"),
        "{types}"
    );
    assert!(
        types.contains("func (p *T_App__Box) Fields() []flickrt.Value { return p.f[:] }"),
        "{types}"
    );
    // `Tagged Int a` unboxes its concrete `Int` field and keeps `a` as `flickrt.Value`.
    assert!(
        types.contains("type T_App__Tagged struct {\n\tf1 flickrt.Value\n\tf0 int32\n}"),
        "{types}"
    );
    assert!(
        go.contains("var l_n int32 = l_t.(*T_App__Tagged).f0"),
        "{go}"
    );
}

#[test]
fn derived_eq_ord_hash_show_use_unboxed_fields_without_reboxing() {
    let out = build(
        "module App exposing (main)\n\n\
         type Item = Empty | Num Int Float | Flag Bool String\n  deriving Eq, Ord, Hash, Show\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () =\n  let a = Num 1 2.5\n  let b = Flag true \"x\"\n  \
         Platform.stdout_line (show (eq a b, compare a b, hash a, hash b, show a, show b))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    // Direct scalar instance methods avoid calling boxed `F_Instances__...` functions.
    assert!(!go.contains("F_Instances__Eq_x20_"), "{go}");
    assert!(!go.contains("F_Instances__Ord_x20_"), "{go}");
    assert!(!go.contains("F_Instances__Hash_x20_"), "{go}");
    assert!(!go.contains("F_Instances__Show_x20_"), "{go}");
    assert!(go.contains("flickrt.EqFloat("), "{go}");
    assert!(go.contains("flickrt.CompareInt("), "{go}");
    assert!(go.contains("flickrt.CompareFloat("), "{go}");
    assert!(go.contains("flickrt.CompareBool("), "{go}");
    assert!(go.contains("flickrt.CompareString("), "{go}");
    assert!(go.contains("flickrt.HashInt("), "{go}");
    assert!(go.contains("flickrt.HashFloat("), "{go}");
    assert!(go.contains("flickrt.HashBool("), "{go}");
    assert!(go.contains("flickrt.HashString("), "{go}");
    assert!(go.contains("flickrt.ShowInt("), "{go}");
    assert!(go.contains("flickrt.ShowFloat("), "{go}");
    assert!(go.contains("flickrt.ShowBool("), "{go}");
    assert!(go.contains("flickrt.ShowString("), "{go}");
    assert!(go.contains("flickrt.ConcatString("), "{go}");
    // No unboxing casts inside derived methods since fields are already typed.
    let derived = go
        .split("func F_App__main(")
        .next()
        .expect("derived section");
    assert!(!derived.contains("flickrt.AsInt("), "{derived}");
    assert!(!derived.contains("flickrt.AsFloat("), "{derived}");
    assert!(!derived.contains("flickrt.AsBool("), "{derived}");
    assert!(!derived.contains("flickrt.AsString("), "{derived}");
}

#[test]
fn curried_constructors_use_nil_safe_field_unboxing() {
    let out = build(
        "module App exposing (main)\n\n\
         type Pair = Pair Int Float\n\n\
         make : Int -> Float -> Pair\n\
         let make = Pair\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () =\n  let partial = Pair 1\n  \
         let items = List.map partial [2.0, 3.0]\n  \
         Platform.stdout_line (show (List.length items, List.length [make 1 2.0]))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    // Curried/value constructor wrappers use nil-safe `Field*` so `tagOf` probes stay safe.
    assert!(go.contains("flickrt.FieldInt("), "{go}");
    assert!(go.contains("flickrt.FieldFloat("), "{go}");
    assert!(go.contains("New_T_App__Pair("), "{go}");
}

#[test]
fn patterns_handle_destructure_neg_zero_literal_and_mixed_or_bindings() {
    let out = build(
        "module App exposing (main)\n\n\
         type Pair = Pair Int Float\n\
         type Mix a = Num Int | Any a\n\n\
         destr : Pair -> Float\n\
         let destr p =\n  let Pair n x = p\n  Int.to_float n + x\n\n\
         is_neg_zero : Pair -> Bool\n\
         let is_neg_zero p =\n  match p with\n  | Pair 0 -0.0 -> true\n  | Pair _ _ -> false\n\n\
         extract : Mix Int -> Int\n\
         let extract m =\n  match m with\n  | Num n | Any n -> n\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (destr (Pair 2 3.5), is_neg_zero (Pair 0 (-0.0)), extract (Num 4)))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let go = file(&out.files, "m_App.go");

    // Single-constructor `let` destructuring reads typed fields directly.
    assert!(go.contains("var l_n int32 = l_p.(*T_App__Pair).f0"), "{go}");
    assert!(
        go.contains("var l_x float64 = l_p.(*T_App__Pair).f1"),
        "{go}"
    );
    // Mixed Or-pattern (`Num Int` native `int32` vs `Any a` boxed `flickrt.Value`) falls back cleanly to `flickrt.Value`.
    assert!(go.contains("var l_n flickrt.Value"), "{go}");
}

#[test]
fn optimized_release_ir_preserves_scalar_field_unboxing() {
    let mut ir = lower_text(
        "App",
        "module App exposing (main, area, measure)\n\n\
         type Shape = Dot | Circle Float | Rect Float Float\n  deriving Eq, Ord, Hash, Show\n\n\
         area : Shape -> Float\n\
         let area s =\n  match s with\n  | Dot -> 0.0\n  | Circle r -> r * r\n  | Rect w h -> w * h\n\n\
         measure : Float -> (Float, Int)\n\
         let measure x = (area (Rect x (x + 1.0)), hash (Circle x))\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (measure 2.0))\n",
    )
    .expect("lowers");
    let exported = |name: &str| matches!(name, "main" | "area" | "measure");
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
    let types = file(&out.files, "own_types.go");
    let go = file(&out.files, "m_App.go");
    assert!(
        types.contains("type T_App__Rect struct {\n\tf0 float64\n\tf1 float64\n}"),
        "{types}"
    );
    assert!(go.contains("flickrt.HashFloat("), "{go}");
    assert!(!go.contains("F_Instances__Hash_x20_Float"), "{go}");
}
