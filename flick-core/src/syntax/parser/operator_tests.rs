use super::tests::{errors, sexp};

#[test]
fn left_pipe_is_weakest_and_right_associative() {
    assert_eq!(
        sexp("let x = f <| g <| a || b"),
        "(LetDecl let (VarPat x) = (Block (BinExpr (NameExpr f) <| (BinExpr (NameExpr g) <| \
         (BinExpr (NameExpr a) || (NameExpr b))))))"
    );
    // the left side of `<|` is an application, as in `f a x`
    assert!(
        sexp("let y = List.map f <| xs")
            .contains("(BinExpr (AppExpr (NameExpr List . map) (NameExpr f)) <| (NameExpr xs))")
    );
    assert!(
        sexp("let z = f <| fun x -> x + 1")
            .contains("(BinExpr (NameExpr f) <| (FunExpr fun (VarPat x) ->")
    );
}

#[test]
fn composition_binds_tighter_than_every_other_operator() {
    assert_eq!(
        sexp("let h = f >> g >> h"),
        "(LetDecl let (VarPat h) = (Block (BinExpr (BinExpr (NameExpr f) >> (NameExpr g)) >> \
         (NameExpr h))))"
    );
    assert_eq!(
        sexp("let h = f << g << h"),
        "(LetDecl let (VarPat h) = (Block (BinExpr (NameExpr f) << (BinExpr (NameExpr g) << \
         (NameExpr h)))))"
    );
    assert!(sexp("let h = List.map f >> List.filter p").contains(
        "(BinExpr (AppExpr (NameExpr List . map) (NameExpr f)) >> \
             (AppExpr (NameExpr List . filter) (NameExpr p)))"
    ));
    assert!(
        sexp("let h = x |> f >> g ** 2").contains(
            "(BinExpr (NameExpr x) |> (BinExpr (BinExpr (NameExpr f) >> (NameExpr g)) **"
        )
    );
    assert!(
        sexp("let h = await f >> g")
            .contains("(PrefixExpr await (BinExpr (NameExpr f) >> (NameExpr g)))")
    );
}

#[test]
fn pipes_of_both_directions_do_not_mix() {
    assert_eq!(errors("let a = x |> f <| y"), ["FLICK-S007 |>"]);
    assert_eq!(errors("let b = f <| x |> g"), ["FLICK-S007 |>"]);
    assert_eq!(errors("let c = f >> g << h"), ["FLICK-S007 >>"]);
    assert_eq!(errors("let d = f << g >> h"), ["FLICK-S007 >>"]);
    assert!(errors("let e = (x |> f) <| y").is_empty());
    assert!(errors("let g = f <| (x |> g)").is_empty());
    assert!(errors("let h = (f >> g) << h").is_empty());
    assert!(errors("let i = x |> f >> g").is_empty());
    assert!(errors("let j = f <| g >> h").is_empty());
}

#[test]
fn a_line_that_starts_with_a_new_operator_continues() {
    assert!(
        sexp("let x =\n  f\n  <| g\n  <| y")
            .contains("(BinExpr (NameExpr f) <| (BinExpr (NameExpr g) <| (NameExpr y)))")
    );
    assert!(sexp("let h =\n  f\n  >> g").contains("(BinExpr (NameExpr f) >> (NameExpr g))"));
}
