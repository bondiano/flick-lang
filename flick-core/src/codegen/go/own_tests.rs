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
         type Shape = Dot | Circle Float | Rect Float Float\n\n\
         area : Shape -> Float\n\
         let area shape =\n  match shape with\n  | Dot -> 0.0\n  | Circle r -> r\n  | Rect w h -> w * h\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (area (Rect 2.0 3.0), area (Circle 1.0), Some 1))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let file = |name: &str| {
        let found = out.files.iter().find(|(file, _)| file.ends_with(name));
        found.map(|(_, go)| go.as_str()).expect(name)
    };
    let (go, types) = (file("m_App.go"), file("own_types.go"));
    // the type is the tag: Go tells the constructor, and a field is read where it is
    assert!(go.contains("if Is_T_App__Rect(l_shape) {"), "{go}");
    assert!(
        go.contains("var l_h flickrt.Value = l_shape.(*T_App__Rect).f[1]"),
        "{go}"
    );
    assert!(
        go.contains("New_T_App__Rect(float64(2e0), float64(3e0))"),
        "{go}"
    );
    // one without fields is the one value of its tag, among values of other Go types
    assert!(go.contains("if flickrt.IsData(l_shape, \"Dot#"), "{go}");
    // a constructor of a built-in type is made by the runtime too: it stays as it was
    assert!(go.contains("flickrt.NewData(\"Some\", "), "{go}");
    assert!(!go.contains("flickrt.NewData(\"Rect#"), "{go}");
    // declared once for the program, with what the runtime reads and builds it by
    assert!(
        types.contains("type T_App__Rect struct{ f [2]flickrt.Value }"),
        "{types}"
    );
    assert!(
        types.contains("func (p *T_App__Rect) Fields() []flickrt.Value { return p.f[:] }"),
        "{types}"
    );
    assert_eq!(types.matches("flickrt.Owned(\"").count(), 2, "{types}");
    assert!(!types.contains("Dot"), "{types}");
}

#[test]
fn a_record_under_a_constructor_lies_in_its_value() {
    let out = build(
        "module App exposing (main)\n\n\
         type Shape = Dot | Box { w : Int; h : Int; name : String }\n\n\
         area : Shape -> Int\n\
         let area shape =\n  match shape with\n  | Dot -> 0\n  | Box { w; h } -> w * h\n\n\
         wider : Shape -> Shape\n\
         let wider shape =\n  match shape with\n  | Dot -> Dot\n  | Box box -> Box { box with h = box.h + 1 }\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () =\n  let r = { w = 1; h = 1; name = \"r\" }\n  \
         Platform.stdout_line (show (area (wider (Box { name = \"a\"; h = 2; w = 3 })), area (Box r)))\n",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let file = |name: &str| {
        let found = out.files.iter().find(|(file, _)| file.ends_with(name));
        found.map(|(_, go)| go.as_str()).expect(name)
    };
    let (go, types) = (file("m_App.go"), file("own_types.go"));
    // one object: the header of a record over the values, in canonical label order
    assert!(
        types.contains(
            "type T_App__Box struct {\n\trec  flickrt.Record\n\tf    [3]flickrt.Value\n\tself [1]flickrt.Value\n}"
        ),
        "{types}"
    );
    assert!(
        types.contains("var S_T_App__Box = flickrt.ShapeOf(\"h\", \"name\", \"w\")"),
        "{types}"
    );
    // the list of its one field is kept in the value: none is made where the runtime asks
    assert!(
        types.contains("func (p *T_App__Box) Fields() []flickrt.Value { return p.self[:] }"),
        "{types}"
    );
    // a literal gives its values to their canonical places ("h", "name", "w"), whatever order it writes them in
    assert!(go.contains("New_T_App__Box(int32(2), t"), "{go}");
    // `{ box with … }` is a copy in a new value with a place set: no record apart
    assert!(go.contains("With_T_App__Box(l_box).Set(0, "), "{go}");
    assert!(!go.contains("flickrt.With("), "{go}");
    // a label of the pattern is a place; the record taken whole is a pointer into the value
    assert!(go.contains("l_shape.(*T_App__Box).f[0]"), "{go}");
    assert!(
        go.contains("var l_box flickrt.Value = flickrt.Value(&l_shape.(*T_App__Box).rec)"),
        "{go}"
    );
    // read through the name, a label is a place too
    assert!(go.contains("flickrt.FieldAt(l_box, 0)"), "{go}");
    // any other record is copied in
    assert!(go.contains("Of_T_App__Box(l_r)"), "{go}");
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
