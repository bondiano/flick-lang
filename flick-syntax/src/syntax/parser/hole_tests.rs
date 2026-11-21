use super::tests::{errors, sexp};

#[test]
fn expression_holes_parse_in_application_arguments_only() {
    assert_eq!(
        sexp("let g = f a _ c"),
        "(LetDecl let (VarPat g) = (Block (AppExpr (NameExpr f) (NameExpr a) (HoleExpr _) (NameExpr c))))"
    );
    assert_eq!(
        sexp("let g = x |> f a _ c"),
        "(LetDecl let (VarPat g) = (Block (BinExpr (NameExpr x) |> (AppExpr (NameExpr f) (NameExpr a) (HoleExpr _) (NameExpr c)))))"
    );
    assert_eq!(
        sexp("let g = f a _ c <| x"),
        "(LetDecl let (VarPat g) = (Block (BinExpr (AppExpr (NameExpr f) (NameExpr a) (HoleExpr _) (NameExpr c)) <| (NameExpr x))))"
    );
    assert_eq!(
        sexp("let g = f (g _) _"),
        "(LetDecl let (VarPat g) = (Block (AppExpr (NameExpr f) (ParenExpr ( (AppExpr (NameExpr g) (HoleExpr _)) )) (HoleExpr _))))"
    );
    assert_eq!(
        sexp("let g =\n  f\n    a\n    _\n    c\n"),
        "(LetDecl let (VarPat g) = (Block (AppExpr (NameExpr f) (NameExpr a) (HoleExpr _) (NameExpr c))))"
    );
    assert_eq!(errors("let a = f _ _"), ["FLICK-S017 _"]);
    assert_eq!(errors("let a = f a _ b _"), ["FLICK-S017 _"]);
    assert_eq!(errors("let a = f _ (g _) _"), ["FLICK-S017 _"]);
    for bad in [
        "let a = x |> _",
        "let a = _ |> f x",
        "let a = f [_, x]",
        "let a = f { field = _ }",
        "let a = _ x",
        "let a = _",
        "let a = f (_)",
        "let a = (_ + 1)",
        "let a = _.name",
        "let a = f _.name",
        "let a = f _?",
    ] {
        assert_eq!(errors(bad), ["FLICK-S018 _"], "{bad}");
    }
    // Wildcard patterns and type holes remain unchanged.
    assert_eq!(errors("let _ = 1"), Vec::<String>::new());
    assert_eq!(errors("let f = fun _ -> 1"), Vec::<String>::new());
    assert_eq!(errors("let x = (1 : _)"), Vec::<String>::new());
}
