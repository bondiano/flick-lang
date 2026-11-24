//! The Go of constructors of the program's own types.

use super::tests::{app, build};

#[test]
fn a_nullary_constructor_stores_its_evidence_instead_of_instantiating_data() {
    let go = app(r"module App exposing (main)

class Gate a =
  allow : a -> Bool

instance Gate Int =
  let allow value = value > 0

type Flag =
  | Gate Int => Flag : Flag
  | Gate Int => OtherFlag : Flag

equal : Flag -> Flag -> Bool
let equal left right = left == right

main : Unit ->{Host.Console; AppError} Unit
let main () = Platform.stdout_line (show (equal Flag OtherFlag))
");
    // Evidence is part of the constructor's storage, not a request to
    // instantiate an already constructed (and non-callable) data value.
    assert!(!go.contains("flickrt.Inst("), "{go}");
    assert!(!go.contains("flickrt.Nullary(\"Flag#"), "{go}");
    assert!(go.contains("flickrt.NewData(\"Flag#"), "{go}");
    assert_eq!(go.matches("I_App__Gate_x20_Int)").count(), 2, "{go}");
}

#[test]
fn a_constructor_given_as_a_function_captures_its_evidence() {
    let go = app(r"module App exposing (main)

class Gate a =
  allow : a -> Bool

instance Gate Int =
  let allow value = value > 0

type Box = Gate a => Box a : Box

constructor : Unit -> (Int -> Box)
let constructor () = Box

allowed : Box -> Bool
let allowed box =
  match box with
  | Box value -> allow value

main : Unit ->{Host.Console; AppError} Unit
let main () = Platform.stdout_line (show (allowed (constructor () 1)))
");
    assert!(!go.contains("flickrt.Inst("), "{go}");
    assert!(go.contains("flickrt.NewData(\"Box#"), "{go}");
    assert!(go.contains(", t1, I_App__Gate_x20_Int)"), "{go}");
}

#[test]
fn a_constructor_with_fields_of_the_program_has_a_go_type_of_its_own() {
    let out = build(
        "module App exposing (main)\n\n\
         type Shape = Dot | Circle Float | Rect Float Float | Group (List Shape)\n\n\
         area : Shape -> Float\n\
         let rec area shape =\n  match shape with\n  | Dot -> 0.0\n  | Circle r -> r\n  | Rect w h -> w * h\n  | Group _ -> 0.0\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (area (Rect 2.0 3.0), area (Circle 1.0), Some 1))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let file = |name: &str| {
        let found = out.files.iter().find(|(file, _)| file.ends_with(name));
        found.map(|(_, go)| go.as_str()).expect(name)
    };
    let (go, types) = (file("m_App.go"), file("own_types.go"));
    // the type is the tag: Go tells the constructor, and scalar fields stay unboxed
    assert!(go.contains("if Is_T_App__Rect(l_shape) {"), "{go}");
    assert!(
        go.contains("var l_h float64 = l_shape.(*T_App__Rect).f1"),
        "{go}"
    );
    assert!(
        go.contains("New_T_App__Rect(float64(2e0), float64(3e0))"),
        "{go}"
    );
    // one without fields has a zero-size Go type of its own: no string comparison
    assert!(go.contains("if Is_T_App__Dot(l_shape) {"), "{go}");
    assert!(!go.contains("flickrt.IsData("), "{go}");
    // a constructor of a built-in type is made by the runtime too: it stays as it was
    assert!(go.contains("flickrt.NewData(\"Some\", "), "{go}");
    assert!(!go.contains("flickrt.NewData(\"Rect#"), "{go}");
    // declared once for the program, with typed scalar fields and runtime boundary adapters
    assert!(types.contains("type T_App__Dot struct{}"), "{types}");
    assert!(
        types.contains("type T_App__Rect struct {\n\tf0 float64\n\tf1 float64\n}"),
        "{types}"
    );
    assert!(
        types.contains(
            "func (p *T_App__Rect) Fields() []flickrt.Value { return []flickrt.Value{p.f0, p.f1} }"
        ),
        "{types}"
    );
    assert!(
        types.contains("func (p *T_App__Rect) EqualConstructed(other flickrt.Value, strictZero bool) (bool, bool)"),
        "{types}"
    );
    assert_eq!(types.matches("flickrt.Owned(\"").count(), 4, "{types}");
}

#[test]
fn a_record_under_a_constructor_lies_in_its_value() {
    let out = build(
        "module App exposing (main)\n\n\
         type Shape = Dot | Box { w : Int; h : Int; name : String } | Group (List Shape)\n\n\
         area : Shape -> Int\n\
         let area shape =\n  match shape with\n  | Dot -> 0\n  | Box { w; h } -> w * h\n  | Group _ -> 0\n\n\
         wider : Shape -> Shape\n\
         let wider shape =\n  match shape with\n  | Dot -> Dot\n  | Box box -> Box { box with h = box.h + 1 }\n  | Group xs -> Group xs\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () =\n  let r = { w = 1; h = 1; name = \"r\" }\n  \
         Platform.stdout_line (show (area (wider (Box { name = \"a\"; h = 2; w = 3 })), area (Box r)))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let file = |name: &str| {
        let found = out.files.iter().find(|(file, _)| file.ends_with(name));
        found.map(|(_, go)| go.as_str()).expect(name)
    };
    let (go, types, records) = (
        file("m_App.go"),
        file("own_types.go"),
        file("record_types.go"),
    );
    // The constructor struct holds the closed record shape directly by value,
    // with scalar fields unboxed and no flickrt.Record header.
    assert!(
        types.contains("type T_App__Box struct {\n\trec R_h_name_w_"),
        "{types}"
    );
    assert!(
        types.contains(
            "func (p *T_App__Box) Fields() []flickrt.Value { return []flickrt.Value{p.rec} }"
        ),
        "{types}"
    );
    assert!(
        types.contains("func New_T_App__Box(rec R_h_name_w_"),
        "{types}"
    );
    assert!(
        types.contains("func Of_T_App__Box(r flickrt.Value) flickrt.Value"),
        "{types}"
    );
    assert!(
        types.contains("func (p *T_App__Box) EqualConstructed(other flickrt.Value, strictZero bool) (bool, bool)"),
        "{types}"
    );
    assert!(!types.contains("S_T_App__Box"), "{types}");
    assert!(!types.contains("With_T_App__Box"), "{types}");
    // The closed record shape keeps canonical label order ("h", "name", "w")
    // and packs pointer-bearing fields (`string`) before `int32`.
    assert!(
        records.contains("var k_R_h_name_w_")
            && records.contains("= []string{\"h\", \"name\", \"w\"}"),
        "{records}"
    );
    assert!(
        records.contains("\tf1 string\n\tf0 int32\n\tf2 int32\n"),
        "{records}"
    );
    // Literal passes the closed record struct by value in source evaluation order
    // (f1="a", f0=2, f2=3) directly to New_T_App__Box.
    assert!(
        go.contains("New_T_App__Box(")
            && go.contains("R_h_name_w_")
            && go.contains("{f1: \"a\", f0: int32(2), f2: int32(3)}"),
        "{go}"
    );
    // `{ box with h = box.h + 1 }` copies the struct on stack, updates `.f0` directly,
    // and constructs `New_T_App__Box` with no flickrt.With or flickrt.FieldAt.
    assert!(go.contains("l_box.f0"), "{go}");
    assert!(!go.contains("flickrt.With("), "{go}");
    assert!(!go.contains("flickrt.FieldAt("), "{go}");
    assert!(!go.contains("flickrt.Field("), "{go}");
    // Pattern matching reads directly through `.rec.f0` and `.rec.f2` as `int32`.
    assert!(go.contains("l_shape.(*T_App__Box).rec.f0"), "{go}");
    assert!(go.contains("l_shape.(*T_App__Box).rec.f2"), "{go}");
    assert!(go.contains("var l_box R_h_name_w_"), "{go}");
    // Passing an existing typed record calls `New_T_App__Box(l_r)` directly by value.
    assert!(go.contains("New_T_App__Box(l_r)"), "{go}");
}

#[test]
fn generic_owner_transport_keeps_the_default_fiber_owner() {
    let go = app(
        "module App exposing (main, forward)\nforward : (Unit ->{r} a) ->{r} a\nlet forward body = body ()\nmain : Unit ->{} Unit\nlet main () = forward (fun () -> ())\n",
    );
    assert!(go.contains("flickrt.ReadOwnerEvidence("), "{go}");
    assert!(go.contains("flickrt.WithOwnerEvidence("), "{go}");
    assert!(
        !go.contains("flickrt.Within("),
        "compiled forwarding transports only inert evidence: {go}"
    );
}

#[test]
fn wildcard_tailrec_carry_is_a_valid_go_local() {
    let go = app(
        "module App exposing (spin, main)\nspin : Unit -> Unit\nlet rec spin () = spin ()\nmain : Unit -> Unit\nlet main () = spin ()\n",
    );
    let variable = go
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let declaration = line.strip_prefix("var ")?;
            declaration
                .contains("flickrt.Value = t")
                .then(|| declaration.split_whitespace().next().expect("loop carry"))
        })
        .expect("wildcard loop-carried temporary");
    assert!(
        go.contains(&format!("_ = {variable}")),
        "an ignored loop parameter still declares a Go local: {go}"
    );
}

#[test]
fn a_payload_free_constructor_has_a_zero_size_go_type_and_no_string_comparison() {
    let out = build(
        "module App exposing (main)\n\n\
         type Tree = Leaf | Node Tree Int Tree\n\n\
         depth : Tree -> Int\n\
         let rec depth tree =\n  match tree with\n  | Leaf -> 0\n  | Node l _ r -> 1 + depth l + depth r\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (depth (Node Leaf 1 Leaf), (None : Option Int), LT))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let file = |name: &str| {
        let found = out.files.iter().find(|(file, _)| file.ends_with(name));
        found.map(|(_, go)| go.as_str()).expect(name)
    };
    let (go, types) = (file("m_App.go"), file("own_types.go"));
    // Zero-size type declared once with TagName, empty Fields, Is_T, and Owned registration.
    assert!(types.contains("type T_App__Leaf struct{}"), "{types}");
    assert!(
        types.contains("func (T_App__Leaf) TagName() string { return \"Leaf#"),
        "{types}"
    );
    assert!(
        types.contains("func (T_App__Leaf) Fields() []flickrt.Value { return nil }"),
        "{types}"
    );
    assert!(
        types.contains("func Is_T_App__Leaf(v flickrt.Value) bool {\n\t_, ok := v.(T_App__Leaf)\n\treturn ok\n}"),
        "{types}"
    );
    assert!(types.contains("var _ = flickrt.Owned(\"Leaf#"), "{types}");
    // Construction uses T{} as flickrt.Value, not flickrt.Nullary.
    assert!(go.contains("flickrt.Value(T_App__Leaf{})"), "{go}");
    assert!(!go.contains("flickrt.Nullary(\"Leaf#"), "{go}");
    // Match tests Go types with no string comparison.
    assert!(go.contains("Is_T_App__Leaf(l_tree)"), "{go}");
    assert!(go.contains("Is_T_App__Node(l_tree)"), "{go}");
    assert!(!go.contains("flickrt.IsData("), "{go}");
    assert!(!go.contains("flickrt.IsTag("), "{go}");
    assert!(!go.contains("\"Leaf#"), "{go}");
    assert!(!go.contains("\"Node#"), "{go}");
    // Built-in nullary constructors (`None`, `LT`) keep their runtime values.
    assert!(go.contains("flickrt.Nullary(\"None\")"), "{go}");
    assert!(go.contains("flickrt.Nullary(\"LT#"), "{go}");
    assert!(!types.contains("None"), "{types}");
    assert!(!types.contains("LT"), "{types}");
}

#[test]
fn a_nullary_gadt_witness_retains_its_dictionary_representation() {
    let source = include_str!("../../../../tests/run/codegen/gadt_dicts.fl");
    let out = build(source);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let file = |name: &str| {
        let found = out.files.iter().find(|(file, _)| file.ends_with(name));
        found.map(|(_, go)| go.as_str())
    };
    let go = file("m_App_Main.go").expect("m_App_Main.go");
    // Witness retains its runtime dictionary representation: never a zero-size own type.
    if let Some(types) = file("own_types.go") {
        assert!(!types.contains("Witness"), "{types}");
    }
    assert!(!go.contains("T_App_Main__Witness"), "{go}");
    assert!(
        go.contains("flickrt.NewData(\"Witness#") && go.contains("I_Instances__Show_x20_Int"),
        "{go}"
    );
    assert!(go.contains("flickrt.IsData(l_witness, \"Witness#"), "{go}");
    assert!(go.contains("flickrt.Get(l_witness, 0)"), "{go}");
    assert!(go.contains("flickrt.Method(fb, "), "{go}");
    assert!(go.contains("\"show\""), "{go}");
}

#[test]
fn all_nullary_equality_reads_tags_through_tag_field() {
    let go = app("module App exposing (main)\n\n\
         type Color = Red | Green | Blue\n\n\
         same_color : Color -> Color -> Bool\n\
         let same_color a b = a == b\n\n\
         same_ord : Ordering -> Ordering -> Bool\n\
         let same_ord a b = a == b\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (same_color Red Green, same_ord LT GT))\n");
    // By-value closed all-nullary ADT compares `.tag` directly without boxing.
    assert!(go.contains("return (l_a.tag == l_b.tag)"), "{go}");
    // Erased all-nullary ADT (built-in `Ordering`) still compares through `TagField`.
    assert!(
        go.contains("flickrt.Same(flickrt.TagField(l_a), flickrt.TagField(l_b))"),
        "{go}"
    );
}
