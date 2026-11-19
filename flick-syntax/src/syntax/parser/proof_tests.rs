use super::tests::{errors, sexp};

#[test]
fn a_proof_names_its_goal_after_where_and_its_proof_after_the_equals() {
    let src = "proof right_identity (v : Option a)\nwhere equal (flat_map pure v) v =\n  match v with\n  | None -> refl\n  | Some x -> refl\n";
    let tree = sexp(src);
    assert!(
        tree.starts_with("(ProofDecl proof (VarPat right_identity) (Params"),
        "{tree}"
    );
    assert!(
        tree.contains("where (Goal (AppExpr (NameExpr equal) (ParenExpr"),
        "{tree}"
    );
    assert!(
        tree.contains("= (Block (MatchExpr match (NameExpr v) with"),
        "{tree}"
    );
}

#[test]
fn a_goal_of_types_and_a_premise_parameter() {
    let tree = sexp("proof same where same_type a a = refl\n");
    assert!(
        tree.contains("(SameTypeGoal same_type (NameType a) (NameType a))"),
        "{tree}"
    );
    let tree = sexp("proof step (x : Bool) (h : equal x true) where holds x = h\n");
    assert!(
        tree.contains("(Premise ( (VarPat h) : (Goal (AppExpr (NameExpr equal)"),
        "{tree}"
    );
}

#[test]
fn the_word_proof_stays_a_name() {
    // a value, a parameter and a call named `proof`: no declaration starts
    let tree = sexp("let proof = 1\nlet use proof = proof + 1\n");
    assert!(!tree.contains("ProofDecl"), "{tree}");
    // `proof` alone at the start of a statement is a name too
    assert!(!sexp("let x =\n  proof\n").contains("ProofDecl"));
}

#[test]
fn a_proof_without_its_goal_or_its_proof_says_what_it_lacks() {
    let found = errors("proof p (x : Int) = refl\n");
    assert_eq!(found, ["FLICK-S007 ="], "{found:?}");
    let found = errors("proof p where equal 1 1\n");
    assert_eq!(found.len(), 1, "{found:?}");
}
