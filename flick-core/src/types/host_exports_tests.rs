//! Unit tests for `@export "name"` collection, placement, and IR lowering.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::tests::lower_text;
use crate::source::SourceMap;

fn check_diags(source: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("ExportTest.fl", source.to_owned());
    crate::check(&sources, file)
}

#[test]
fn collects_export_before_signature_between_sig_and_let_in_rec_and_private() {
    let source = "\
module Demo exposing (add, is_even)

@export \"addNumbers\"
add : Int -> Int -> Int
let add a b = a + b

sub : Int -> Int -> Int
@export \"subtractNumbers\"
let sub a b = a - b

rec {
  @export \"isEven\"
  is_even : Int -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Int -> Bool
  @export \"isOdd\"
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
";
    let ir = lower_text("Demo", source).expect("should check and lower cleanly");
    let got: Vec<(&str, &str, &str)> = ir
        .exports
        .host
        .iter()
        .map(|e| {
            let slice = &source[e.span.start as usize..e.span.end as usize];
            (e.name.as_str(), e.export_name.as_str(), slice)
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("add", "addNumbers", "@export \"addNumbers\""),
            ("sub", "subtractNumbers", "@export \"subtractNumbers\""),
            ("is_even", "isEven", "@export \"isEven\""),
            ("is_odd", "isOdd", "@export \"isOdd\""),
        ]
    );
}

#[test]
fn rejects_export_on_non_function_let_destructure_and_foreign() {
    let non_fn = "\
module Demo exposing (answer)

@export \"answer\"
answer : Int
let answer = 42
";
    let diags = check_diags(non_fn);
    let err = diags
        .iter()
        .find(|d| d.severity == Severity::Error && d.code == codes::S014)
        .expect("S014 on non-function let");
    assert!(err.message.contains("top-level function"), "{err:?}");
    let span = err.span.expect("span");
    assert_eq!(
        &non_fn[span.start as usize..span.end as usize],
        "@export \"answer\""
    );

    let destructure = "\
module Demo exposing (..)

@export \"pair\"
let (a, b) = (1, 2)
";
    let diags = check_diags(destructure);
    let err = diags
        .iter()
        .find(|d| d.severity == Severity::Error && d.code == codes::S014)
        .expect("S014 on destructuring let");
    assert!(err.message.contains("named top-level function"), "{err:?}");

    let foreign = "\
module Demo exposing (ext)

@export \"ext\"
ext : Int -> Int
foreign pure ext from \"./ext.mjs\" \"ext\"
";
    let diags = check_diags(foreign);
    let err = diags
        .iter()
        .find(|d| d.severity == Severity::Error && d.code == codes::S014)
        .expect("S014 on foreign declaration");
    assert!(err.message.contains("not a `foreign`"), "{err:?}");
}
