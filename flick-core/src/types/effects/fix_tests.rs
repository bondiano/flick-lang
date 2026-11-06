//! The fix of an E002 adds what a written row lacks right after its `{`.

use crate::diagnostics::codes;
use crate::source::SourceMap;

/// `signature` after the fix of its E002, applied to the text of `program`.
fn fixed(signature: &str, program: &str) -> String {
    let text = format!(
        "module Probe exposing (..)\n\
         type error NotFound = NotFound Float\n\
         service Users =\n  get : Float ->{{; NotFound}} String\n\
         service Log =\n  line : String ->{{}} Unit\n\
         {signature}\n{program}\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.clone());
    let diagnostics = crate::check(&sources, file);
    let edits: Vec<_> = (diagnostics.iter())
        .filter(|d| d.code == codes::E002)
        .flat_map(|d| &d.fixes)
        .flat_map(|f| &f.edits)
        .collect();
    let [edit] = edits[..] else {
        panic!("expected one edit, got {diagnostics:?}");
    };
    let mut out = text;
    out.replace_range(edit.span.range(), &edit.replacement);
    let line = out
        .lines()
        .find(|l| l.starts_with("f :"))
        .expect("the signature");
    line.to_owned()
}

#[test]
fn the_fix_adds_the_service_after_the_brace() {
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed("f : String ->{} Unit", program),
        "f : String ->{Log;} Unit"
    );
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed("f : String ->{; NotFound} Unit", program),
        "f : String ->{Log; NotFound} Unit"
    );
    let program = "let f x =\n  let _ = Users.get 1.0\n  Log.line x";
    assert_eq!(
        fixed("f : String ->{Users; NotFound} Unit", program),
        "f : String ->{Log, Users; NotFound} Unit"
    );
}

#[test]
fn a_row_of_a_variable_gets_the_service_before_it() {
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed("f : forall r. String ->{r} Unit", program),
        "f : forall r. String ->{Log | r} Unit"
    );
}
