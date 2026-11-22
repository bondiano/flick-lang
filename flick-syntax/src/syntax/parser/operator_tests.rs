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
    assert_eq!(
        sexp("let x = a orelse b catch | E -> c"),
        "(LetDecl let (VarPat x) = (Block (CatchExpr (BinExpr (NameExpr a) orelse (NameExpr b)) catch \
         (MatchArm | (ConPat E) -> (Block (NameExpr c))))))"
    );
    assert!(
        sexp("let x = result { a } orelse b")
            .contains("(BinExpr (ResultExpr result { (Block (NameExpr a)) }) orelse (NameExpr b))")
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

#[test]
fn postfix_catch_precedence_nesting_and_layout() {
    // Weaker than all infix operators, including `<|` and `|>`.
    assert_eq!(
        sexp("let x = a |> f catch | E -> b"),
        "(LetDecl let (VarPat x) = (Block (CatchExpr (BinExpr (NameExpr a) |> (NameExpr f)) catch \
         (MatchArm | (ConPat E) -> (Block (NameExpr b))))))"
    );
    assert_eq!(
        sexp("let x = f <| a || b catch | E -> c"),
        "(LetDecl let (VarPat x) = (Block (CatchExpr (BinExpr (NameExpr f) <| (BinExpr (NameExpr a) || (NameExpr b))) \
         catch (MatchArm | (ConPat E) -> (Block (NameExpr c))))))"
    );
    // Recovery pipeline belongs to the arm; outer pipeline requires parentheses.
    assert!(sexp("let x = load id catch | E -> r |> norm").contains(
        "(CatchExpr (AppExpr (NameExpr load) (NameExpr id)) catch \
             (MatchArm | (ConPat E) -> (Block (BinExpr (NameExpr r) |> (NameExpr norm)))))"
    ));
    assert!(sexp("let x = (load id catch | E -> r) |> norm").contains(
        "(BinExpr (ParenExpr ( (CatchExpr (AppExpr (NameExpr load) (NameExpr id)) catch \
             (MatchArm | (ConPat E) -> (Block (NameExpr r)))) )) |> (NameExpr norm))"
    ));
    // Nested catch inside RHS vs parenthesized outer catch.
    assert!(
        sexp("let x = a catch | E -> b catch | F -> c").contains(
            "(CatchExpr (NameExpr a) catch (MatchArm | (ConPat E) -> (Block (CatchExpr (NameExpr b) catch \
             (MatchArm | (ConPat F) -> (Block (NameExpr c)))))))"
        )
    );
    assert!(
        sexp("let x = (a catch | E -> b) catch | F -> c").contains(
            "(CatchExpr (ParenExpr ( (CatchExpr (NameExpr a) catch (MatchArm | (ConPat E) -> (Block (NameExpr b)))) )) \
             catch (MatchArm | (ConPat F) -> (Block (NameExpr c))))"
        )
    );
    // Same-line multi-arm and multiline layout continuation at block column.
    assert!(errors("let x = load id catch | A -> 1 | B -> 2").is_empty());
    assert!(errors("let x =\n  load id catch\n  | A -> 1\n  | B -> 2\n").is_empty());
    assert!(errors("let x =\n  if ok then load 1 else load 2\n  catch\n  | A -> 0\n").is_empty());
    // Layout boundary inside enclosing `match`: outer `| B ->` stays owned by `match`.
    let nested =
        sexp("let x =\n  match v with\n  | A ->\n    load id catch\n    | E -> 0\n  | B -> 1\n");
    assert!(
        nested.contains(
            "(MatchArm | (ConPat A) -> (Block (CatchExpr (AppExpr (NameExpr load) (NameExpr id)) catch \
             (MatchArm | (ConPat E) -> (Block (Literal 0)))))) (MatchArm | (ConPat B) -> (Block (Literal 1)))"
        ),
        "{nested}"
    );
    // Rejected forms: missing bars, shorthand, missing arrow, chained catch without parens.
    for bad in [
        "let x = a catch E -> 0",
        "let x = a catch fallback",
        "let x = a catch | E",
        "let x =\n  a catch\n  | E -> b\n  catch\n  | F -> c\n",
        "let catch = 1",
        "let f () =\n  let mut catch = 1\n  1",
        "let f = fun catch -> 1",
        "let { catch } = r",
    ] {
        let errs = errors(bad);
        assert!(!errs.is_empty(), "expected error for `{bad}`");
        assert!(
            errs.iter().any(|e| e.starts_with("FLICK-S007")),
            "{bad}: {errs:?}"
        );
    }
    // Partial editor inputs recover losslessly without panicking.
    for partial in [
        "let x =\n  load id catch\n",
        "let x =\n  load id catch\n  | \n",
        "let x =\n  load id catch\n  | Not\n",
    ] {
        let mut map = SourceMap::default();
        let parsed = super::parse(partial, map.add("t.fl", partial));
        assert_eq!(parsed.tree.text(partial), partial);
        assert!(!parsed.diagnostics.is_empty(), "{partial}");
    }
}

#[test]
fn catch_remains_valid_as_record_label_member_and_foreign_metadata() {
    let ok = "\
type alias Handler = {
  name : String
  catch : Int -> Int
}

foreign read_text from \"./fs.js\" \"readText\"
  arity 1
  catch FsError.from_js

let demo r =
  let created = {
    name = \"ok\"
    catch = fun n -> n + 1
  }
  let updated = { created with catch = fun n -> n + 2 }
  let {
    name
    catch = handler
  } = updated
  let a = r.catch
  let b = (id r).catch
  let c = { catch = 1 }.catch
  let d = Layer.catch
  (name, handler, a, b, c, d)
";
    assert_eq!(errors(ok), Vec::<String>::new());
}
