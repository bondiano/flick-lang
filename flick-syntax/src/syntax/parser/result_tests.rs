//! Contextual `result { … }` block parsing and recovery.

use super::parse;
use super::tests::{errors, sexp};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;

#[test]
fn a_contextual_result_block_parses_only_before_braces() {
    assert_eq!(
        sexp("let x = result { 1 }\n"),
        "(LetDecl let (VarPat x) = (Block (ResultExpr result { (Block (Literal 1)) })))"
    );
    assert_eq!(
        sexp("let x = result { f () }?\n"),
        "(LetDecl let (VarPat x) = (Block (QuestionExpr (ResultExpr result { (Block (AppExpr (NameExpr f) (UnitExpr ( )))) }) ?)))"
    );
    assert_eq!(
        sexp("let x = g result { 1 }\n"),
        "(LetDecl let (VarPat x) = (Block (AppExpr (NameExpr g) (ResultExpr result { (Block (Literal 1)) }))))"
    );
    assert_eq!(
        sexp("let x = result {}\n"),
        "(LetDecl let (VarPat x) = (Block (ResultExpr result { (Block ) })))"
    );
    // Outside `result { ... }`, `result` stays an ordinary identifier;
    // `(result) { field = 1 }` is a normal record call and `provide result { 1 }`
    // parses `result` as the layer expression.
    for text in [
        "let result = 1\n",
        "let f result = result + 1\n",
        "let x = obj.result\n",
        "let x = (result) { field = 1 }\n",
        "let x = provide result { 1 }\n",
        "let x = while result do { () }\n",
    ] {
        let tree = sexp(text);
        assert!(!tree.contains("ResultExpr"), "{text}: {tree}");
        assert_eq!(errors(text), Vec::<String>::new(), "{text}");
    }
    // Recovery: an unclosed `result {` does not hide later top-level declarations.
    let broken = "let a () = result {\n  let x = 1\n\nlet b = 2\n";
    let parsed = parse(broken, FileId::new(0));
    assert!(!parsed.diagnostics.is_empty());
    assert!(
        parsed.tree.nodes().filter(|n| n.kind == K::LetDecl).count() >= 2,
        "{}",
        sexp(broken)
    );
}
