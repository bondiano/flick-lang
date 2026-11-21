use super::tests::{errors, sexp};
use crate::source::SourceMap;
use crate::syntax::cst::Node;

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

#[test]
fn a_right_chain_past_the_depth_limit_nests_no_deeper() {
    fn depth(node: &Node) -> u32 {
        1 + node.nodes().map(depth).max().unwrap_or(0)
    }
    let src = format!("let x = {}1\nlet y = 2", "f <| ".repeat(1000));
    let mut map = SourceMap::default();
    let parsed = super::parse(&src, map.add("t.fl", src.as_str()));
    assert!(depth(&parsed.tree) <= super::MAX_DEPTH + 8);
    assert_eq!(parsed.tree.text(&src), src);
    let found = errors(&src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("FLICK-S007"), "{found:?}");
}

#[test]
fn orelse_sits_between_pipes_associates_right_and_continues_lines() {
    assert_eq!(
        sexp("let x = a orelse b orelse c"),
        "(LetDecl let (VarPat x) = (Block (BinExpr (NameExpr a) orelse (BinExpr (NameExpr b) orelse (NameExpr c)))))"
    );
    assert_eq!(
        sexp("let x = a |> f orelse b"),
        "(LetDecl let (VarPat x) = (Block (BinExpr (BinExpr (NameExpr a) |> (NameExpr f)) orelse (NameExpr b))))"
    );
    assert_eq!(
        sexp("let x = f <| a orelse b"),
        "(LetDecl let (VarPat x) = (Block (BinExpr (NameExpr f) <| (BinExpr (NameExpr a) orelse (NameExpr b)))))"
    );
    assert_eq!(
        sexp("let x = a orelse b || c"),
        "(LetDecl let (VarPat x) = (Block (BinExpr (NameExpr a) orelse (BinExpr (NameExpr b) || (NameExpr c)))))"
    );
    assert!(
        sexp("let x =\n  a\n  orelse b\n  orelse c")
            .contains("(BinExpr (NameExpr a) orelse (BinExpr (NameExpr b) orelse (NameExpr c)))")
    );
    assert!(
        sexp("let x = a orelse if ok then 1 else 2")
            .contains("(BinExpr (NameExpr a) orelse (IfExpr if")
    );
    assert!(
        sexp("let x = a orelse fun y -> y").contains("(BinExpr (NameExpr a) orelse (FunExpr fun")
    );
    assert_eq!(errors("let x = a orelse"), ["FLICK-S007 "]);
    assert_eq!(errors("let x = a |> f <| b orelse c"), ["FLICK-S007 |>"]);
}

#[test]
fn orelse_is_reserved_as_a_binder_but_allowed_as_a_record_label() {
    assert!(!errors("let orelse = 1").is_empty());
    assert!(!errors("let f orelse = 1").is_empty());
    assert!(!errors("orelse : Int").is_empty());
    assert_eq!(errors("let f { orelse } = 1"), ["FLICK-S007 orelse"]);

    for valid in [
        "type alias R = { orelse : Option Int }",
        "type alias R = {\n  host : String\n  orelse : Option Int\n}",
        "let r = { orelse = Some 1 }",
        "let r = {\n  host = \"a\"\n  orelse = Some 1\n}",
        "let r2 = { r with orelse = None }",
        "let x = r.orelse",
        "let x = (f ()).orelse",
        "let x = { orelse = 1 }.orelse",
        "let x = r.orelse orelse 0",
        "let f { orelse = x } = x",
        "let f {\n  host\n  orelse = x\n} = x",
    ] {
        assert!(errors(valid).is_empty(), "{valid}: {:?}", errors(valid));
    }
    assert!(
        sexp("let x = r.orelse orelse 0")
            .contains("(BinExpr (FieldExpr (NameExpr r) . orelse) orelse (Literal 0))")
    );
}

#[test]
fn a_deep_orelse_right_chain_hits_depth_limit_cleanly() {
    fn depth(node: &Node) -> u32 {
        1 + node.nodes().map(depth).max().unwrap_or(0)
    }
    let src = format!("let x = {}0\nlet y = 1", "a orelse ".repeat(1000));
    let mut map = SourceMap::default();
    let parsed = super::parse(&src, map.add("t.fl", src.as_str()));
    assert!(depth(&parsed.tree) <= super::MAX_DEPTH + 8);
    assert_eq!(parsed.tree.text(&src), src);
    let found = errors(&src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("FLICK-S007"), "{found:?}");
}
