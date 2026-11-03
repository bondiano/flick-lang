use super::Target;
use super::tests::fixture_of;
use crate::diagnostics::Diagnostic;

#[test]
fn kind_hover_documents_every_builtin_and_row_constructor() {
    let source = "module App.Kinds exposing (..)

type Sample
  (a : Type)
  (services : ServiceRow)
  (errors : ErrorRow)
  (captures : CaptureRow)
  (fields : FieldRow)
  (label : Symbol)
  (region : Region)
  (constraint : Constraint)
  (row : Row Type)
  (higher_row : Row (Type -> Type))
";
    let f = fixture_of(&[("src/App/Kinds.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Kinds");
    for (needle, code, phrase) in [
        ("a : Type", "Type", "ordinary value types"),
        ("services : ServiceRow", "ServiceRow", "service identities"),
        ("errors : ErrorRow", "ErrorRow", "errors a computation"),
        (
            "captures : CaptureRow",
            "CaptureRow",
            "regions a value may capture",
        ),
        ("fields : FieldRow", "FieldRow", "record field rows"),
        ("label : Symbol", "Symbol", "type-level string labels"),
        ("region : Region", "Region", "region identity"),
        (
            "constraint : Constraint",
            "Constraint",
            "type-class constraints",
        ),
        ("row : Row Type", "Row k", "whose entries have kind `k`"),
        (
            "higher_row : Row (Type -> Type)",
            "Row k",
            "whose entries have kind `k`",
        ),
    ] {
        let start =
            source.find(needle).expect("kind annotation") + needle.find(':').expect("present") + 2;
        let start = u32::try_from(start).expect("small");
        let width = if code == "Row k" { 3 } else { code.len() };
        let (markdown, range) = ide.hover(file, start + 1).expect("kind hover");
        assert_eq!(range, (start, start + u32::try_from(width).expect("small")));
        assert!(
            markdown.starts_with(&format!("```flick\n{code}\n```\n\n")),
            "{markdown}"
        );
        assert!(markdown.contains(phrase), "{markdown}");
    }
    let nested_type =
        u32::try_from(source.find("Row (Type -> Type)").expect("present") + 5).expect("present");
    assert!(
        ide.hover(file, nested_type + 1)
            .expect("nested kind hover")
            .0
            .contains("ordinary value types")
    );
    let arrow = u32::try_from(source.find("Type -> Type").expect("present") + "Type ".len())
        .expect("present");
    let (markdown, range) = ide.hover(file, arrow).expect("kind arrow hover");
    assert_eq!(range, (arrow, arrow + 2));
    assert!(
        markdown.starts_with("```flick\nType -> Type\n```"),
        "{markdown}"
    );
    assert!(
        markdown.contains("A kind arrow describes a type constructor"),
        "{markdown}"
    );
}

#[test]
fn named_kind_hover_shows_its_declaration_and_member_kind() {
    let source = "module App.Kinds exposing (..)

/// Controls the state of a switch.
kind Mode = Off | On

type Switch (mode : Mode)
";
    let f = fixture_of(&[("src/App/Kinds.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Kinds");
    for needle in ["Mode =", "mode : Mode"] {
        let start = u32::try_from(
            source.find(needle).expect("present") + needle.rfind("Mode").expect("present"),
        )
        .expect("present");
        let (markdown, range) = ide.hover(file, start + 1).unwrap_or_else(|| {
            panic!(
                "named kind hover at {needle}: {:?}",
                ide.symbol_at(file, start + 1)
            )
        });
        assert_eq!(range, (start, start + 4));
        assert!(
            markdown.starts_with("```flick\nkind Mode = Off | On\n```"),
            "{markdown}"
        );
        assert!(
            markdown.contains("Controls the state of a switch."),
            "{markdown}"
        );
    }
    let start = u32::try_from(source.find("Off |").expect("present")).expect("present");
    let (markdown, range) = ide.hover(file, start + 1).expect("kind member hover");
    assert_eq!(range, (start, start + 3));
    assert!(
        markdown.starts_with("```flick\nOff : Mode\n```"),
        "{markdown}"
    );
    assert!(
        markdown.contains("A member of the finite kind `Mode`."),
        "{markdown}"
    );
}

#[test]
fn imported_kind_annotation_hovers_and_navigates_to_its_declaration() {
    let owner = "module App.Mode exposing (Mode(..), Type(..))

/// The available operating modes.
kind Mode = Off | On

kind Type = Small | Large
";
    let user = "module App.Main exposing (..)

import App.Mode exposing (Mode)

type Switch (mode : Mode)

type Indexed (index : App.Mode.Type)
";
    let f = fixture_of(&[("src/App/Mode.fl", owner), ("src/App/Main.fl", user)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Main");
    let start = u32::try_from(user.find("mode : Mode").expect("present") + "mode : ".len())
        .expect("present");
    let (symbol, range) = ide.symbol_at(file, start + 1).expect("kind symbol");
    assert_eq!(range, (start, start + 4));
    assert_eq!(
        ide.definition(&symbol).expect("kind definition").target,
        Target::File(f.file("App.Mode"))
    );
    let (markdown, _) = ide.hover(file, start + 1).expect("kind hover");
    assert!(
        markdown.starts_with("```flick\nkind Mode = Off | On\n```"),
        "{markdown}"
    );
    assert!(
        markdown.contains("The available operating modes."),
        "{markdown}"
    );
    let start = u32::try_from(user.rfind("Type").expect("present")).expect("present");
    let (markdown, range) = ide.hover(file, start + 1).expect("qualified kind hover");
    assert_eq!(range, (start, start + 4));
    assert!(
        markdown.starts_with("```flick\nkind Type = Small | Large\n```"),
        "{markdown}"
    );
    assert!(
        markdown.contains("A finite kind defined by the listed members."),
        "{markdown}"
    );
}
