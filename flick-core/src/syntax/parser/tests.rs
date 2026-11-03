use super::*;
use crate::source::SourceMap;
use crate::syntax::cst::Element;

/// Compact shape of the tree: node kinds and token texts, no trivia; nodes
/// with one child are shown as `Kind:child` to keep expectations short.
fn sexp(src: &str) -> String {
    fn walk(node: &Node, src: &str) -> String {
        let parts: Vec<String> = node
            .children
            .iter()
            .filter_map(|c| match c {
                Element::Node(n) => Some(walk(n, src)),
                Element::Token(t) if t.kind.is_trivia() => None,
                Element::Token(t) => Some(t.text(src).to_owned()),
            })
            .collect();
        format!("({:?} {})", node.kind, parts.join(" "))
    }
    let parsed = parse_ok(src);
    parsed
        .tree
        .nodes()
        .map(|n| walk(n, src))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_ok(src: &str) -> Parse {
    let parsed = parse_str(src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    parsed
}

fn parse_str(src: &str) -> Parse {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    parse(src, file)
}

fn errors(src: &str) -> Vec<String> {
    parse_str(src)
        .diagnostics
        .iter()
        .map(|d| format!("{} {}", d.code, &src[d.span.expect("span").range()]))
        .collect()
}

#[test]
fn operator_precedence_and_associativity() {
    assert_eq!(
        sexp("let x = a |> f || b && c == d ++ e + g * h ** i ** j"),
        "(LetDecl let (VarPat x) = (Block (BinExpr (NameExpr a) |> (BinExpr (NameExpr f) || \
         (BinExpr (NameExpr b) && (BinExpr (NameExpr c) == (BinExpr (NameExpr d) ++ \
         (BinExpr (NameExpr e) + (BinExpr (NameExpr g) * (BinExpr (NameExpr h) ** \
         (BinExpr (NameExpr i) ** (NameExpr j))))))))))))"
    );
    assert!(
        sexp("let l = a |> f |> g")
            .contains("(BinExpr (BinExpr (NameExpr a) |> (NameExpr f)) |> (NameExpr g))")
    );
    assert!(
        sexp("let r = a || b || c")
            .contains("(BinExpr (NameExpr a) || (BinExpr (NameExpr b) || (NameExpr c)))")
    );
    assert_eq!(
        sexp("let y = -x ** 2 - f a * b"),
        "(LetDecl let (VarPat y) = (Block (BinExpr (PrefixExpr - (BinExpr (NameExpr x) ** \
         (Literal 2))) - (BinExpr (AppExpr (NameExpr f) (NameExpr a)) * (NameExpr b)))))"
    );
    assert_eq!(errors("let z = a < b < c"), ["FLICK-S007 <"]);
}

#[test]
fn layout_separates_continues_and_ends_blocks() {
    let src = "let f x =\n  let a = g x\n    y\n  a\n  |> h\nlet next = 1\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat f) (Params (VarPat x)) = (Block (LetDecl let (VarPat a) = \
         (Block (AppExpr (NameExpr g) (NameExpr x) (NameExpr y)))) (BinExpr (NameExpr a) |> \
         (NameExpr h))))\n(LetDecl let (VarPat next) = (Block (Literal 1)))"
    );
}

#[test]
fn else_if_chains_and_try_with_at_block_column() {
    let src = "let f x =\n  if a then\n    1\n  else if b then\n    2\n  else\n    3\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat f) (Params (VarPat x)) = (Block (IfExpr if (NameExpr a) then \
         (Block (Literal 1)) else (IfExpr if (NameExpr b) then (Block (Literal 2)) else \
         (Block (Literal 3))))))"
    );
    let src = "let g =\n  try\n    load ()\n  with\n  | E -> 0\n";
    assert!(sexp(src).contains("(TryExpr try (Block (AppExpr (NameExpr load) (UnitExpr ( )))) with (MatchArm | (ConPat E) -> (Block (Literal 0))))"));
}

#[test]
fn if_without_else_ends_at_its_then_branch() {
    let src = "let f x =\n  if a then\n    g x\n  h x\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat f) (Params (VarPat x)) = (Block (IfExpr if (NameExpr a) then \
         (Block (AppExpr (NameExpr g) (NameExpr x)))) (AppExpr (NameExpr h) (NameExpr x))))"
    );
}

#[test]
fn mid_line_braces_align_on_their_first_item() {
    let src = "let r = { a = 1\n          b = 2 }\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat r) = (Block (RecordExpr { (RecordField a = (Block (Literal 1))) \
         (RecordField b = (Block (Literal 2))) })))"
    );
}

#[test]
fn effect_arrow_needs_adjacent_brace() {
    assert_eq!(
        sexp("f : A ->{r; e} B"),
        "(Signature f : (ArrowType (NameType A) -> (EffectAnnot { (Row (NameType r)) ; \
         (Row (NameType e)) }) (NameType B)))"
    );
    assert_eq!(
        sexp("f : A ->{; e} B"),
        "(Signature f : (ArrowType (NameType A) -> (EffectAnnot { (Row ) ; (Row (NameType e)) }) \
         (NameType B)))"
    );
    assert_eq!(
        sexp("f : A -> { x : B }"),
        "(Signature f : (ArrowType (NameType A) -> (RecordType { (RecordTypeField x : \
         (NameType B)) })))"
    );
}

#[test]
fn type_lists_variants_and_alias_names_a_type() {
    assert_eq!(
        sexp("type W a = Wrap (List a) a^c"),
        "(TypeDecl type W (TypeParam a) = (Variant Wrap (ParenType ( (AppType (NameType List) \
         (NameType a)) )) (CaptureType (NameType a) ^ (NameType c))))"
    );
    assert_eq!(
        sexp("type alias Pair a = (a, a)"),
        "(TypeDecl type alias Pair (TypeParam a) = (TupleType ( (NameType a) , (NameType a) )))"
    );
    assert_eq!(
        sexp("type error Failed e = Failed e Float\ntype error Timeout"),
        "(ErrorDecl type error Failed (TypeParam e) = (Variant Failed (NameType e) \
         (NameType Float)))\n(ErrorDecl type error Timeout)"
    );
    assert_eq!(
        errors("type T = Some of a\ntype error E = F\n"),
        ["FLICK-S010 of", "FLICK-S007 F"]
    );
}

#[test]
fn dedented_closers_close_their_bracket() {
    let src = "let f =\n  g (fun x ->\n    x\n)\n";
    assert!(sexp(src).contains("(ParenExpr ( (FunExpr fun (VarPat x) -> (Block (NameExpr x))) ))"));
}

#[test]
fn errors_do_not_hide_later_declarations() {
    let src = "let a = (1 +\nlet b = ]\ntype = 1\nlet ok = 1\nlet c x =\n      x\n    y\n";
    assert_eq!(
        errors(src),
        [
            "FLICK-S007 ",
            "FLICK-S007 ]",
            "FLICK-S007 =",
            "FLICK-S009 y"
        ]
    );
}

#[test]
fn deep_nesting_is_an_error_not_a_stack_overflow() {
    let src = format!("let x = {}1", "(".repeat(100_000));
    let found = errors(&src);
    assert!(found[0].starts_with("FLICK-S007"), "{found:?}");
    assert_eq!(parse_str(&src).tree.text(&src), src);
}

#[test]
fn cli_block_clauses() {
    let src = "let c =\n  cli \"greet\" {\n    description \"Say hi\"\n    /// loud\n    flag times \"n\" \"t\" : Option Float = 1\n    arg name : String\n    subcommand other\n    handle fun { times; name } -> run times name\n  }\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat c) = (Block (CliExpr cli \"greet\" { (Block \
         (CliClause description (Literal \"Say hi\")) \
         (CliClause flag times \"n\" \"t\" : (AppType (NameType Option) (NameType Float)) = (Literal 1)) \
         (CliClause arg name : (NameType String)) \
         (CliClause subcommand (NameExpr other)) \
         (CliClause handle (FunExpr fun (RecordPat { (RecordPatField times) ; (RecordPatField name) }) -> \
         (Block (AppExpr (NameExpr run) (NameExpr times) (NameExpr name)))))) })))"
    );
    // a broken clause is reported once and the next clause still parses
    assert_eq!(
        errors("let c = cli \"x\" { flag : Bool; bogus 1; arg y : T }"),
        ["FLICK-S007 :", "FLICK-S007 bogus"]
    );
}

#[test]
fn sql_captures_take_a_name_a_field_path_or_parentheses() {
    let tree = sexp("let q = sql { from u in T; where u.a == $x.y && u.b == $(f x); limit $M.n }");
    assert_eq!(
        tree,
        "(LetDecl let (VarPat q) = (Block (SqlExpr sql { (Block (SqlClause from u in (NameExpr T)) ; \
         (SqlClause where (BinExpr (BinExpr (FieldExpr (NameExpr u) . a) == (SqlCapture $ (FieldExpr (NameExpr x) . y))) && \
         (BinExpr (FieldExpr (NameExpr u) . b) == (SqlCapture $ (ParenExpr ( (AppExpr (NameExpr f) (NameExpr x)) )))))) ; \
         (SqlClause limit (SqlCapture $ (NameExpr M . n)))) })))"
    );
    assert_eq!(
        errors("let q = sql { from u in T; where u.a == $ x }\nlet r = $x\n"),
        // a space after `$`, and `$` outside `sql { … }`
        ["FLICK-S007 x", "FLICK-S007 $"]
    );
}

#[test]
fn list_patterns_end_with_a_rest() {
    assert_eq!(
        sexp("let [x, ...rest] = xs"),
        "(LetDecl let (ListPat [ (VarPat x) , (RestPat ... (VarPat rest)) ]) = (Block (NameExpr xs)))"
    );
    assert_eq!(errors("let [...rest, x] = xs"), ["FLICK-S007 ,"]);
    assert_eq!(errors("let [...] = xs"), ["FLICK-S007 ]"]);
}

#[test]
fn let_assert_takes_a_pattern_and_a_message() {
    assert_eq!(
        sexp("let assert Some x = v as \"no x\""),
        "(LetDecl let assert (ConPat Some (VarPat x)) = (Block (NameExpr v)) as \"no x\")"
    );
    assert_eq!(errors("let rec assert f x = x"), ["FLICK-S007 assert"]);
    assert_eq!(errors("let x = v as \"no\""), ["FLICK-S007 as"]);
}

#[test]
fn do_blocks_bind_guard_and_sequence() {
    let src = "let p =\n  do {\n    Some x <- a\n    if x > 0\n    let y = x\n    log y\n    pure (x, y)\n  }\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat p) = (Block (DoExpr do { (Block (BindStmt (ConPat Some (VarPat x)) <- \
         (NameExpr a)) (GuardStmt if (BinExpr (NameExpr x) > (Literal 0))) (LetDecl let (VarPat y) = \
         (Block (NameExpr x))) (AppExpr (NameExpr log) (NameExpr y)) (AppExpr (NameExpr pure) \
         (TupleExpr ( (NameExpr x) , (NameExpr y) )))) })))"
    );
    assert!(
        sexp("let q = do { (a, b) <- f (fun x -> x); if c then g a else h b }")
            .contains("(BindStmt (TuplePat ( (VarPat a) , (VarPat b) )) <- (AppExpr (NameExpr f)")
    );
    assert!(sexp("let q = do { if c then g else h }").contains("(IfExpr if"));
}

#[test]
fn and_joins_bindings_of_a_do_group_only() {
    assert_eq!(
        sexp("let p = do { x <- a and (y, z) <- f b; pure (x + y) }"),
        "(LetDecl let (VarPat p) = (Block (DoExpr do { (Block (BindGroup (BindStmt (VarPat x) <- \
         (NameExpr a)) and (BindStmt (TuplePat ( (VarPat y) , (VarPat z) )) <- (AppExpr (NameExpr f) \
         (NameExpr b)))) ; (AppExpr (NameExpr pure) (ParenExpr ( (BinExpr (NameExpr x) + (NameExpr y)) )))) })))"
    );
    // elsewhere `and` is a name
    assert!(sexp("let q = do { x <- Bool.and a b; and x y }").contains("(NameExpr and)"));
    assert!(sexp("let r = do { x <- f (g and h); pure x }").contains("(NameExpr and)"));
}

#[test]
fn ranges_bind_between_comparisons_and_arithmetic() {
    assert_eq!(
        sexp("let r = 0..n - 1"),
        "(LetDecl let (VarPat r) = (Block (RangeExpr (Literal 0) .. (BinExpr (NameExpr n) - (Literal 1)))))"
    );
    assert!(
        sexp("let b = i < 0..<n")
            .contains("(BinExpr (NameExpr i) < (RangeExpr (Literal 0) ..< (NameExpr n)))")
    );
    assert_eq!(
        sexp("let s = f (1..)"),
        "(LetDecl let (VarPat s) = (Block (AppExpr (NameExpr f) (ParenExpr ( (RangeExpr (Literal 1) ..) )))))"
    );
    assert_eq!(errors("let r = 1..2..3"), ["FLICK-S007 .."]);
}

#[test]
fn larrow_outside_do_and_do_without_braces_are_errors() {
    assert_eq!(errors("let f x =\n  y <- load x\n  y\n"), ["FLICK-S013 <-"]);
    assert_eq!(errors("let b = a < -1"), Vec::<String>::new());
    assert_eq!(errors("let f x =\n  do\n    x\n"), ["FLICK-S010 do"]);
}

#[test]
fn for_takes_braced_or_inline_clauses_and_yields() {
    assert_eq!(
        sexp("let s = for { x <- xs; if x > 0 } yield x * 2"),
        "(LetDecl let (VarPat s) = (Block (ForExpr for { (Block (BindStmt (VarPat x) <- (NameExpr xs)) ; \
         (GuardStmt if (BinExpr (NameExpr x) > (Literal 0)))) } yield (BinExpr (NameExpr x) * (Literal 2)))))"
    );
    assert_eq!(
        sexp("let l = [for x <- xs if p x; y <- ys yield (x, y)]"),
        "(LetDecl let (VarPat l) = (Block (ListExpr [ (ForExpr for (Block (BindStmt (VarPat x) <- \
         (NameExpr xs)) (GuardStmt if (AppExpr (NameExpr p) (NameExpr x))) ; (BindStmt (VarPat y) <- \
         (NameExpr ys))) yield (TupleExpr ( (NameExpr x) , (NameExpr y) ))) ])))"
    );
    let src = "let s =\n  for {\n    x <- xs\n  }\n  yield x\n";
    assert!(sexp(src).contains("} yield (NameExpr x)"), "{}", sexp(src));
}

#[test]
fn for_do_runs_a_body_inline_indented_or_braced() {
    assert!(
        sexp("let f xs = for x <- xs do log x")
            .contains("do (Block (AppExpr (NameExpr log) (NameExpr x)))")
    );
    let src = "let f xs =\n  for { x <- xs } do\n    log x\n    log x\n";
    assert!(sexp(src).contains("} do (Block (AppExpr"), "{}", sexp(src));
    assert!(sexp("let f xs = for x <- xs do { log x; log x }").contains("do { (Block"));
}

#[test]
fn token_columns_follow_unicode_crlf_and_multiline_tokens() {
    for (literal, plus_col) in [("r\"é\r\n🙂\"", 3), ("\"\"\"\r\n  é🙂\r\n  \"\"\"", 6)] {
        let text = format!("/*é🙂*/let a = {literal} + 1\r\n/*α\r\nβ*/  let b = 2\r\n");
        let mut sources = SourceMap::default();
        let file = sources.add("layout.fl", &text);
        let (raw, errors) = lex(&text, file);
        assert!(errors.is_empty(), "{errors:?}");
        let parser = Parser::new(&text, file, &raw);
        let layout: Vec<_> = parser
            .toks
            .iter()
            .filter(|t| t.kind != K::Eof)
            .map(|t| {
                (
                    &text[t.start as usize..t.end as usize],
                    t.col,
                    t.line_start,
                    t.line_indent,
                )
            })
            .collect();
        assert_eq!(
            layout,
            [
                ("let", 6, true, 6),
                ("a", 10, false, 6),
                ("=", 12, false, 6),
                (literal, 14, false, 6),
                ("+", plus_col, false, 6),
                ("1", plus_col + 2, false, 6),
                ("let", 5, true, 5),
                ("b", 9, false, 5),
                ("=", 11, false, 5),
                ("2", 13, false, 5),
            ],
            "{text}"
        );
    }
}

#[test]
fn unicode_comment_columns_preserve_record_layout() {
    for newline in ["\n", "\r\n"] {
        let src = format!(
            "let r = {{ /*é🙂*/ a = 1{newline}{}b = 2 }}{newline}",
            " ".repeat(17)
        );
        assert_eq!(
            sexp(&src),
            "(LetDecl let (VarPat r) = (Block (RecordExpr { (RecordField a = (Block (Literal 1))) \
             (RecordField b = (Block (Literal 2))) })))"
        );
        assert_eq!(parse_ok(&src).tree.text(&src), src);
    }
}

#[test]
fn a_component_is_a_let_of_its_props_and_body() {
    assert_eq!(
        sexp("component label (p : P) = text p.name\n"),
        "(LetDecl component (VarPat label) (Block (ComponentExpr (AnnotPat ( (VarPat p) : \
         (NameType P) )) = (Block (AppExpr (NameExpr text) (FieldExpr (NameExpr p) . name))))))"
    );
    let src = "component counter (p : P) =\n  let n = 1\n  show n\nlet x = 1\n";
    assert_eq!(
        sexp(src),
        "(LetDecl component (VarPat counter) (Block (ComponentExpr (AnnotPat ( (VarPat p) : \
         (NameType P) )) = (Block (LetDecl let (VarPat n) = (Block (Literal 1))) \
         (AppExpr (NameExpr show) (NameExpr n))))))\n(LetDecl let (VarPat x) = (Block (Literal 1)))"
    );
    // the word stays a name elsewhere
    assert_eq!(
        errors("let render component setup = component setup\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_server_fn_is_a_let_and_server_names_its_implementation() {
    assert_eq!(
        sexp("server fn place order = order\n"),
        "(LetDecl server fn (VarPat place) (Params (VarPat order)) = (Block (NameExpr order)))"
    );
    assert_eq!(
        sexp("let x = [server place, server Api.place]\n"),
        "(LetDecl let (VarPat x) = (Block (ListExpr [ (ServerExpr server (NameExpr place)) , \
         (ServerExpr server (NameExpr Api . place)) ])))"
    );
    // the word stays a name elsewhere
    assert_eq!(
        errors(
            "let f server fn = server\nlet g server = (server)\nlet h = call server double 21\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn attributes_stand_before_declarations_and_constructors() {
    let src = "@lang int\ntype Int\n@lang flat_map\nflat_map : a -> a\nclass Monad m =\n  @lang pure\n  pure : a -> m a\ntype Option a = @lang none None | @lang some Some a\n";
    assert_eq!(
        sexp(src),
        "(Attribute @ lang int)\n(TypeDecl type Int)\n(Attribute @ lang flat_map)\n\
         (Signature flat_map : (ArrowType (NameType a) -> (NameType a)))\n\
         (ClassDecl class Monad (TypeParam m) = (Members (Attribute @ lang pure) \
         (Signature pure : (ArrowType (NameType a) -> (AppType (NameType m) (NameType a))))))\n\
         (TypeDecl type Option (TypeParam a) = (Variant (Attribute @ lang none) None) | \
         (Variant (Attribute @ lang some) Some (NameType a)))"
    );
}
