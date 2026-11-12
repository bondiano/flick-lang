//! The Go of constructors of the program's own types.

use super::tests::build;

#[test]
fn a_constructor_with_fields_of_the_program_has_a_go_type_of_its_own() {
    let out = build(
        "module App exposing (main)\n\n\
         type Shape = Dot | Circle Float | Rect Float Float\n\n\
         area : Shape -> Float\n\
         let area shape =\n  match shape with\n  | Dot -> 0.0\n  | Circle r -> r\n  | Rect w h -> w * h\n\n\
         main : Unit ->{; AppError} Unit\n\
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
