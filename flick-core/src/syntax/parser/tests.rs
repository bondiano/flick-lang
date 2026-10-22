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
        sexp("type error Failed e = Failed e Number\ntype error Timeout"),
        "(ErrorDecl type error Failed (TypeParam e) = (Variant Failed (NameType e) \
         (NameType Number)))\n(ErrorDecl type error Timeout)"
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
    let src = "let c =\n  cli \"greet\" {\n    description \"Say hi\"\n    /// loud\n    flag times \"n\" \"t\" : Option Number = 1\n    arg name : String\n    subcommand other\n    handle fun { times; name } -> run times name\n  }\n";
    assert_eq!(
        sexp(src),
        "(LetDecl let (VarPat c) = (Block (CliExpr cli \"greet\" { (Block \
         (CliClause description (Literal \"Say hi\")) \
         (CliClause flag times \"n\" \"t\" : (AppType (NameType Option) (NameType Number)) = (Literal 1)) \
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
