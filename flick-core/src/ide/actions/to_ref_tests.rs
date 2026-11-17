//! "Keep `x` in a `Ref`" for a `let mut` variable a `task` or a function
//! uses: the result checks clean.

use super::tests::{action, applied, main};
use crate::diagnostics::Severity;

/// The program after the action on `name` where `let mut` binds it,
/// checked to have no errors.
fn kept(text: &str, name: &str) -> String {
    let (f, text) = main(text);
    let at = f.at("App.Main", &format!("mut {name}"), 0) + 4;
    let a = action(&f, at, at, "keep").expect("the action");
    assert_eq!(a.title, format!("keep `{name}` in a `Ref`"));
    let fixed = applied(&text, &a);
    let checked = main(&fixed).0;
    let errors: Vec<_> = (checked.ide().analysis.diagnostics.iter())
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{fixed}\n{errors:?}");
    fixed
}

#[test]
fn a_variable_a_task_reads_becomes_a_cell() {
    let fixed = kept(
        "module App.Main exposing (run)\n\nrun : Unit ->{} Int\nlet run () =\n  let mut count = 1\n  count = count + 1\n  let later = task { count }\n  await later\n",
        "count",
    );
    assert_eq!(
        fixed,
        "module App.Main exposing (run)\n\nrun : Unit ->{} Int\nlet run () =\n  let count = Ref.make 1\n  Ref.set count (Ref.get count + 1)\n  let later = task { Ref.get count }\n  await later\n"
    );
}

#[test]
fn its_signature_becomes_a_cell_and_an_argument_is_parenthesized() {
    let fixed = kept(
        "module App.Main exposing (run)\n\nrun : Unit ->{} String\nlet run () =\n  total : Int\n  let mut total = 0\n  let report = fun () -> show total\n  total = 2\n  report ()\n",
        "total",
    );
    assert!(
        fixed.contains("  total : Ref Int\n  let total = Ref.make 0\n"),
        "{fixed}"
    );
    assert!(fixed.contains("fun () -> show (Ref.get total)"), "{fixed}");
    assert!(fixed.contains("  Ref.set total 2\n"), "{fixed}");
}

#[test]
fn a_type_after_the_name_becomes_a_cell() {
    let fixed = kept(
        "module App.Main exposing (run)\n\nrun : Unit ->{} Bool\nlet run () =\n  let mut seen : Bool = false\n  let mark = fun () -> seen = true\n  mark ()\n  seen\n",
        "seen",
    );
    assert!(
        fixed.contains("  let seen : Ref Bool = Ref.make false\n"),
        "{fixed}"
    );
    assert!(fixed.contains("fun () -> Ref.set seen true\n"), "{fixed}");
    assert!(fixed.ends_with("  Ref.get seen\n"), "{fixed}");
}

/// Used only by its own function, the variable needs no cell.
#[test]
fn a_variable_its_function_alone_uses_is_left() {
    let (f, _) = main(
        "module App.Main exposing (run)\n\nrun : Int -> Int\nlet run n =\n  let mut count = n\n  count = count + 1\n  count\n",
    );
    let at = f.at("App.Main", "count", 0);
    assert!(action(&f, at, at, "keep").is_none());
}
