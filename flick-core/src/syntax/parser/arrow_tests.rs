//! Arrows and the braces after them, and `lazy`.

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

/// `lazy { … }` is a block, as `task { … }`; `lazy e` binds as `await e`.
#[test]
fn lazy_takes_a_block_or_an_operand() {
    assert_eq!(
        sexp("let l = lazy { load id }"),
        "(LetDecl let (VarPat l) = (Block (LazyExpr lazy { (Block (AppExpr (NameExpr load) (NameExpr id))) })))"
    );
    assert_eq!(
        sexp("let l = lazy load id"),
        "(LetDecl let (VarPat l) = (Block (LazyExpr lazy (AppExpr (NameExpr load) (NameExpr id)))))"
    );
    assert_eq!(
        sexp("let v = f lazy { x }"),
        "(LetDecl let (VarPat v) = (Block (AppExpr (NameExpr f) (LazyExpr lazy { (Block (NameExpr x)) }))))"
    );
}
