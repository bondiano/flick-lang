//! Arrows and the braces after them.

use super::tests::{errors, parse_str, sexp};

/// `->{ x : B }` reads as the arrow to a record it was meant as, reported
/// with the space that makes it one: a row never starts `name :`.
#[test]
fn a_record_touching_its_arrow_is_reported_with_a_space() {
    let src = "f : A ->{ x : B }\n";
    assert_eq!(errors(src), ["FLICK-S007 {"]);
    let parsed = parse_str(src);
    let fix = &parsed.diagnostics[0].fixes[0];
    assert!(fix.safe);
    let [edit] = &fix.edits[..] else {
        panic!("one edit: {fix:?}")
    };
    let mut fixed = src.to_owned();
    fixed.replace_range(edit.span.range(), &edit.replacement);
    assert_eq!(fixed, "f : A -> { x : B }\n");
    assert_eq!(
        sexp(&fixed),
        "(Signature f : (ArrowType (NameType A) -> (RecordType { (RecordTypeField x : \
         (NameType B)) })))"
    );
    assert!(errors("f : A ->{r; e} B\n").is_empty());
}
