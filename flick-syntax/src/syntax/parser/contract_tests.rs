use super::tests::{errors, sexp};

const CONSUME: &str = "consume : Int -> Int -> Int
let consume (available : Int) (used : Int)
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used
";

#[test]
fn a_contract_stands_between_the_parameters_and_the_equals() {
    let tree = sexp(CONSUME);
    assert!(errors(CONSUME).is_empty(), "{:?}", errors(CONSUME));
    assert!(
        tree.contains("where (Contract (Requires require (ParenExpr"),
        "{tree}"
    );
    assert!(
        tree.contains("(Ensures ensure (ParenExpr ( (FunExpr fun (VarPat remaining)"),
        "{tree}"
    );
    assert!(tree.contains(")) = (Block (BinExpr"), "{tree}");
}

#[test]
fn one_entry_inline_and_entries_in_any_order() {
    let src = "let inc (x : Int) where require (x < 10) = x + 1\nlet dec (x : Int)\nwhere\n  ensure (fun y -> y < x)\n  require (x > 0)\n=\n  x - 1\n";
    let tree = sexp(src);
    assert!(errors(src).is_empty(), "{:?}", errors(src));
    assert!(
        tree.contains("where (Contract (Requires require (ParenExpr"),
        "{tree}"
    );
    assert!(
        tree.contains("where (Contract (Ensures ensure (ParenExpr")
            && tree.contains("(Requires require (ParenExpr ( (BinExpr (NameExpr x) >"),
        "{tree}"
    );
}

#[test]
fn the_words_of_a_contract_stay_names() {
    let src = "let require = 1\nlet ensure x = x\nlet use = ensure require\n";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
    assert!(!sexp(src).contains("Contract"));
}

#[test]
fn a_where_without_entries_says_what_it_lacks() {
    let errs = errors("let f (x : Int) where = x\n");
    assert_eq!(errs, ["FLICK-S007 ="], "{errs:?}");
}
