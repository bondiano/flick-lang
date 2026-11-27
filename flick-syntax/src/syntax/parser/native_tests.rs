use crate::source::SourceMap;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;

fn parse_src(src: &str) -> (Node, Vec<String>) {
    let mut map = SourceMap::default();
    let file = map.add("test.fl", src);
    let parsed = parse(src, file);
    let diags = parsed
        .diagnostics
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    (parsed.tree, diags)
}

#[test]
fn parses_single_line_let_with_native_overrides() {
    let src = "\
hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
  @native \"Math.hypot\" when js
  @native \"math.Hypot\" when go
";
    let (tree, diags) = parse_src(src);
    assert!(diags.is_empty(), "{diags:?}");
    let let_decl = tree
        .nodes()
        .find(|n| n.kind == K::LetDecl)
        .expect("let decl");
    let overrides: Vec<_> = let_decl
        .nodes()
        .filter(|n| n.kind == K::NativeOverride)
        .collect();
    assert_eq!(overrides.len(), 2);
    assert_eq!(
        overrides[0]
            .nodes()
            .find(|n| n.kind == K::WhenCond)
            .expect("first when cond")
            .text(src)
            .trim(),
        "js"
    );
    assert_eq!(
        overrides[1]
            .nodes()
            .find(|n| n.kind == K::WhenCond)
            .expect("second when cond")
            .text(src)
            .trim(),
        "go"
    );
}

#[test]
fn parses_multiline_let_body_and_adapter_metadata_and_boolean_when() {
    let src = "\
hypot : Float -> Float -> Float
let hypot a b =
  let s = a * a + b * b
  Float.sqrt s
  @native \"./math.mjs\" \"hypot\" when js && !browser && (node || effect-adapter)
    this
    variadic
  @native global \"math.Hypot\" when go && !cgo
  @native inline \"(a, b) => Math.hypot(a, b)\" when bun
";
    let (tree, diags) = parse_src(src);
    assert!(diags.is_empty(), "{diags:?}");
    let let_decl = tree
        .nodes()
        .find(|n| n.kind == K::LetDecl)
        .expect("let decl");
    let block = let_decl
        .nodes()
        .find(|n| n.kind == K::Block)
        .expect("let block");
    assert_eq!(block.nodes().count(), 2);
    let overrides: Vec<_> = let_decl
        .nodes()
        .filter(|n| n.kind == K::NativeOverride)
        .collect();
    assert_eq!(overrides.len(), 3);
    let meta = overrides[0]
        .nodes()
        .find(|n| n.kind == K::ForeignMeta)
        .expect("foreign meta");
    assert_eq!(meta.nodes().count(), 2);
    assert_eq!(
        overrides[0]
            .nodes()
            .find(|n| n.kind == K::WhenCond)
            .expect("when cond")
            .text(src)
            .trim(),
        "js && !browser && (node || effect-adapter)"
    );
}

#[test]
fn rejects_local_and_invalid_native_override_placements() {
    let local = "\
let outer x =
  let inner y = y + 1
    @native \"Math.abs\" when js
  inner x
";
    let (_, diags) = parse_src(local);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("FLICK-S014") && d.contains("top-level")),
        "{diags:?}"
    );

    let destructure = "\
let (a, b) = (1, 2)
  @native \"foo\" when js
";
    let (_, diags) = parse_src(destructure);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("FLICK-S014") && d.contains("destructuring")),
        "{diags:?}"
    );

    let standalone_attr = "\
@native \"Math.hypot\" when js
let hypot a b = a + b
";
    let (_, diags) = parse_src(standalone_attr);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("FLICK-S014") && d.contains("indented after")),
        "{diags:?}"
    );

    let stray_bang = "let f x = !x\n";
    let (_, diags) = parse_src(stray_bang);
    assert!(
        diags
            .iter()
            .any(|d| d.contains("FLICK-S010") && d.contains("not")),
        "{diags:?}"
    );
}

/// A `type` says where it is the backend's own with `@native when`, after its
/// constructors and `deriving`; it names no binding.
#[test]
fn parses_the_native_condition_of_a_type() {
    let src = "\
type Box a =
  | Box a
  deriving Eq
  @native when js || go
";
    let (tree, diags) = parse_src(src);
    assert!(diags.is_empty(), "{diags:?}");
    let decl = tree
        .nodes()
        .find(|n| n.kind == K::TypeDecl)
        .expect("type decl");
    let ov = decl
        .nodes()
        .find(|n| n.kind == K::NativeOverride)
        .expect("override");
    assert!(ov.nodes().any(|n| n.kind == K::WhenCond), "{ov:#?}");
    assert!(!ov.nodes().any(|n| n.kind == K::ForeignSource), "{ov:#?}");
    let (_, diags) = parse_src("type Box a =\n  | Box a\n  @native \"x\" when js\n");
    assert!(
        diags.iter().any(|d| d.contains("names no binding")),
        "{diags:?}"
    );
}
