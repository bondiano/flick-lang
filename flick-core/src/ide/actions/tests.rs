use super::super::tests::{Fixture, fixture_of};
use super::Action;
use crate::diagnostics::{Diagnostic, Severity};

const SHAPES: &str = "module App.Shapes exposing (..)

type Shape = Circle Float | Rect Float Float | Empty
";

/// `App.Main` of `main` (with `App.Shapes` beside it) and its text.
fn main(main: &str) -> (Fixture, String) {
    let f = fixture_of(&[("src/App/Shapes.fl", SHAPES), ("src/App/Main.fl", main)]);
    (f, main.to_owned())
}

/// The action titled `title` for the range `start..end` of `App.Main`.
fn action(f: &Fixture, start: u32, end: u32, title: &str) -> Option<Action> {
    let file = f.file("App.Main");
    f.ide()
        .actions(file, start, end)
        .into_iter()
        .find(|a| a.title.starts_with(title))
}

fn applied(text: &str, action: &Action) -> String {
    let mut edits = action.edits.clone();
    edits.sort_by_key(|e| e.span.start);
    let mut out = text.to_owned();
    for e in edits.iter().rev() {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.replacement);
    }
    out
}

#[test]
fn add_signature_writes_the_inferred_type_of_a_value() {
    let (f, text) = main("module App.Main exposing (limit)\n\nlet limit = [1, 2]\n");
    let at = f.at("App.Main", "limit =", 0);
    let a = action(&f, at, at, "add the signature").expect("action");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (limit)\n\nlimit : List Int\nlet limit = [1, 2]\n"
    );
}

#[test]
fn unresolved_class_can_be_imported() {
    let src =
        "module App.Main exposing (Expr(..))\n\ntype Expr a =\n  | Numeric a => Add a a : Expr a\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "Numeric", 0);
    let a = action(&f, at, at, "import `Numeric`").expect("class import");
    assert!(applied(&text, &a).contains("import Numeric exposing (Numeric)"));

    let existing = src.replace("\ntype Expr", "\nimport Numeric\n\ntype Expr");
    let (f, text) = main(&existing);
    let at = f.at("App.Main", "Numeric a", 0);
    let a = action(&f, at, at, "import `Numeric`").expect("extend import");
    assert!(applied(&text, &a).contains("import Numeric exposing (Numeric)"));

    let classes = "module App.Classes exposing (Named)\n\nclass Named a =\n  name : a -> String\n";
    let needs = "module App.Main exposing (Box(..))\n\nimport App.Classes\n\ntype Box a =\n  | Named a => Box a : Box a\n";
    let f = fixture_of(&[("src/App/Classes.fl", classes), ("src/App/Main.fl", needs)]);
    let at = f.at("App.Main", "Named a", 0);
    let a = action(&f, at, at, "import `Named`").expect("project class import");
    assert!(applied(needs, &a).contains("import App.Classes exposing (Named)"));
}

#[test]
fn todo_signature_of_gadt_interpreter_is_checked_before_offering() {
    let src = "module App.Main exposing (evalT)\n\nimport Numeric exposing (Numeric)\n\ntype ExprT a =\n  | IntLitT Int : ExprT Int\n  | BoolLitT Bool : ExprT Bool\n  | Numeric a => Add a a : ExprT a\n  | Equals a (a -> b) : ExprT b\n\nevalT : todo\nlet evalT e =\n  match e with\n  | IntLitT n -> n\n  | BoolLitT b -> b\n  | Equals x f -> f x\n  | Add x y -> x + y\n";
    let (f, text) = main(src);
    assert!(
        f.ide()
            .analysis
            .diagnostics
            .iter()
            .any(|d| d.code.to_string() == "FLICK-T024" && d.severity == Severity::Error)
    );
    assert!(
        !f.ide()
            .analysis
            .diagnostics
            .iter()
            .any(|d| d.code.to_string() == "FLICK-T004")
    );
    let at = f.at("App.Main", "todo", 0);
    let a = action(&f, at, at, "replace `todo`").expect("GADT signature");
    let fixed = applied(&text, &a);
    assert!(fixed.contains("evalT : forall a. ExprT a -> a"), "{fixed}");
    let checked = fixture_of(&[("src/App/Main.fl", &fixed)]);
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );

    let partial = src.replace("evalT : todo", "evalT : ExprT a -> todo");
    let (f, text) = main(&partial);
    let at = f.at("App.Main", "todo", 0);
    let a = action(&f, at, at, "replace `todo`").expect("GADT result type");
    let fixed = applied(&text, &a);
    assert!(fixed.contains("evalT : ExprT a -> a"));
    let checked = fixture_of(&[("src/App/Main.fl", &fixed)]);
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary)
    );
}

#[test]
fn imported_gadt_signature_uses_exposed_type_name() {
    let expr = "module App.Expr exposing (Expr(..))\n\ntype Expr a =\n  | IntLit Int : Expr Int\n  | BoolLit Bool : Expr Bool\n  | Wrap b (b -> a) : Expr a\n";
    let main = "module App.Main exposing (eval)\n\nimport App.Expr exposing (Expr(..))\n\neval : todo\nlet eval e =\n  match e with\n  | IntLit n -> n\n  | BoolLit b -> b\n  | Wrap x f -> f x\n";
    let f = fixture_of(&[("src/App/Expr.fl", expr), ("src/App/Main.fl", main)]);
    let at = f.at("App.Main", "todo", 0);
    let a = action(&f, at, at, "replace `todo`").expect("inferred signature");
    let fixed = applied(main, &a);
    assert!(fixed.contains("eval : forall a. Expr a -> a"), "{fixed}");
    assert!(!fixed.contains("App.Expr.Expr"), "{fixed}");

    let aliased = main
        .replace(
            "import App.Expr exposing (Expr(..))",
            "import App.Expr as E",
        )
        .replace("| IntLit", "| E.IntLit")
        .replace("| BoolLit", "| E.BoolLit")
        .replace("| Wrap", "| E.Wrap");
    let explicit = aliased.replace("eval : todo", "eval : forall a. E.Expr a -> a");
    let checked = fixture_of(&[("src/App/Expr.fl", expr), ("src/App/Main.fl", &explicit)]);
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );
    let f = fixture_of(&[("src/App/Expr.fl", expr), ("src/App/Main.fl", &aliased)]);
    let at = f.at("App.Main", "todo", 0);
    let a = action(&f, at, at, "replace `todo`")
        .unwrap_or_else(|| panic!("aliased signature: {:?}", f.ide().analysis.diagnostics));
    assert!(applied(&aliased, &a).contains("eval : forall a. E.Expr a -> a"));
}

#[test]
fn gadt_todo_signature_can_have_a_constant_result() {
    let src = "module App.Main exposing (describe)\n\ntype Expr a =\n  | IntLit Int : Expr Int\n  | BoolLit Bool : Expr Bool\n\ndescribe : todo\nlet describe e =\n  match e with\n  | IntLit _ -> \"int\"\n  | BoolLit _ -> \"bool\"\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "todo", 0);
    let a = action(&f, at, at, "replace `todo`").expect("constant result signature");
    let fixed = applied(&text, &a);
    assert!(
        fixed.contains("describe : forall a. Expr a -> String"),
        "{fixed}"
    );

    let invalid = src.replace("\"bool\"", "true");
    let (f, _) = main(&invalid);
    let at = f.at("App.Main", "todo", 0);
    assert!(action(&f, at, at, "replace `todo`").is_none());
}

#[test]
fn nested_todo_type_is_an_error_with_a_replacement() {
    let src = "module App.Main exposing (wrap)\n\nwrap : Int -> Option todo\nlet wrap x = Some x\n";
    let (f, text) = main(src);
    let diagnostic = f
        .ide()
        .analysis
        .diagnostics
        .iter()
        .find(|d| d.code.to_string() == "FLICK-T024")
        .expect("todo error");
    assert_eq!(diagnostic.severity, Severity::Error);
    let fix = diagnostic
        .fixes
        .iter()
        .find(|f| !f.edits.is_empty())
        .expect("inferred replacement");
    let mut changed = text;
    for edit in fix.edits.iter().rev() {
        changed.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
    }
    assert!(changed.contains("wrap : Int -> Option Int"), "{changed}");
}

#[test]
fn each_todo_type_gets_an_independent_replacement() {
    let src = "module App.Main exposing (wrap, identity)\n\nwrap : Int -> Option todo\nlet wrap x = Some x\n\nidentity : Int -> todo\nlet identity x = x\n";
    let (fixture, _) = main(src);
    let diagnostics: Vec<_> = fixture
        .ide()
        .analysis
        .diagnostics
        .iter()
        .filter(|d| d.code.to_string() == "FLICK-T024")
        .collect();
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    for diagnostic in diagnostics {
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(
            diagnostic.fixes.iter().any(|fix| !fix.edits.is_empty()),
            "{diagnostic:?}"
        );
    }
}

#[test]
fn pattern_match_on_a_variable_lists_its_constructors() {
    let src = "module App.Main exposing (area)\n\nimport App.Shapes exposing (..)\n\narea : Shape -> Float\nlet area shape =\n  shape\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "  shape", 0) + 2;
    let a = action(&f, at, at, "pattern match on `shape`").expect("action");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (area)\n\nimport App.Shapes exposing (..)\n\narea : Shape -> Float\nlet area shape =\n  match shape with\n    | App.Shapes.Circle _ -> todo\n    | App.Shapes.Rect _ _ -> todo\n    | App.Shapes.Empty -> todo\n"
    );
}

#[test]
fn qualify_and_unqualify_rewrite_every_use() {
    let src = "module App.Main exposing (twice)\n\nimport List exposing (map)\n\ntwice : List Float -> List Float\nlet twice xs = map (fun x -> x * 2) (map (fun x -> x) xs)\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "map (fun", 0);
    let a = action(&f, at, at, "qualify `map`").expect("qualify");
    let qualified = applied(&text, &a);
    assert_eq!(
        qualified,
        "module App.Main exposing (twice)\n\nimport List\n\ntwice : List Float -> List Float\nlet twice xs = List.map (fun x -> x * 2) (List.map (fun x -> x) xs)\n"
    );
    let (f, text) = main(&qualified);
    let at = f.at("App.Main", "List.map", 0) + 5;
    let a = action(&f, at, at, "import `map`").expect("unqualify");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (twice)\n\nimport List exposing (map)\n\ntwice : List Float -> List Float\nlet twice xs = map (fun x -> x * 2) (map (fun x -> x) xs)\n"
    );
}

#[test]
fn extract_and_inline_round_trip() {
    let src = "module App.Main exposing (total)\n\ntotal : Float -> Float\nlet total n =\n  let base = n + 1\n  base * (n - 2)\n";
    let (f, text) = main(src);
    let start = f.at("App.Main", "n - 2", 0);
    let a = action(&f, start, start + 5, "extract `value`").expect("extract");
    let extracted = applied(&text, &a);
    assert_eq!(
        extracted,
        "module App.Main exposing (total)\n\ntotal : Float -> Float\nlet total n =\n  let base = n + 1\n  let value = n - 2\n  base * value\n"
    );
    let at = f.at("App.Main", "base =", 0);
    let a = action(&f, at, at, "inline `base`").expect("inline");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (total)\n\ntotal : Float -> Float\nlet total n =\n  (n + 1) * (n - 2)\n"
    );
}

#[test]
fn a_let_mut_variable_is_neither_extracted_from_nor_inlined() {
    let src = "module App.Main exposing (count)\n\ncount : Int -> (Int, Int)\nlet count n =\n  let mut x = n\n  let before = x\n  x = x + 1\n  (before, x * 2)\n";
    let (f, _) = main(src);
    let at = f.at("App.Main", "x =", 1);
    assert!(action(&f, at, at, "inline `x`").is_none(), "the variable");
    let at = f.at("App.Main", "before =", 0);
    assert!(
        action(&f, at, at, "inline `before`").is_none(),
        "a value that reads the variable, used after it changes"
    );
    let start = f.at("App.Main", "x * 2", 0);
    assert!(
        action(&f, start, start + 5, "extract `value`").is_none(),
        "a read moved before its statement"
    );
}
