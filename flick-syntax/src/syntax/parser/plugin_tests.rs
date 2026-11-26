use super::tests::{errors, sexp};
use super::*;
use crate::source::SourceMap;
use crate::syntax::cst::Token;
use crate::syntax::heads::{HeadCatalog, HeadRule};

const IMPORT: &str = "import Pairs exposing (Pair, plugin pairs)\n\n";

fn test_catalog() -> HeadCatalog {
    HeadCatalog::new(vec![
        HeadRule {
            head: "cli".into(),
            module: "Cli".into(),
            implicit: true,
            bare: false,
            declarations: Vec::new(),
            named: false,
            top: false,
            bracket_args: false,
        },
        HeadRule {
            head: "server".into(),
            module: "Server".into(),
            implicit: true,
            bare: true,
            declarations: vec!["fn".into()],
            named: false,
            top: false,
            bracket_args: false,
        },
        HeadRule {
            head: "component".into(),
            module: "Ui".into(),
            implicit: true,
            bare: false,
            declarations: Vec::new(),
            named: true,
            top: false,
            bracket_args: false,
        },
        HeadRule {
            head: "suite".into(),
            module: "Test".into(),
            implicit: true,
            bare: false,
            declarations: Vec::new(),
            named: false,
            top: true,
            bracket_args: true,
        },
        HeadRule {
            head: "test".into(),
            module: "Test".into(),
            implicit: true,
            bare: false,
            declarations: Vec::new(),
            named: false,
            top: true,
            bracket_args: true,
        },
    ])
}

fn parse_str(src: &str) -> Parse {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    parse_with_catalog(src, file, &test_catalog())
}

fn parse_with(src: &str, catalog: &HeadCatalog) -> Parse {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    parse_with_catalog(src, file, catalog)
}

/// The kinds of the nodes from the root down to the first `kind`.
fn find(node: &Node, kind: SyntaxKind) -> Option<&Node> {
    if node.kind == kind {
        return Some(node);
    }
    node.nodes().find_map(|n| find(n, kind))
}

/// The tokens of the body of the first plugin block of `src`.
fn body(src: &str) -> (Parse, Vec<Token>) {
    let parsed = parse_str(src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let block = find(&parsed.tree, K::PluginExpr).expect("a plugin block");
    let body = block.nodes().find(|n| n.kind == K::Block).expect("a body");
    let tokens = body.tokens();
    (parsed, tokens)
}

/// The body tokens of `src` from the first token whose text is `at` on.
fn from(src: &str, tokens: &[Token], at: &str) -> Vec<Token> {
    let i = tokens
        .iter()
        .position(|t| t.text(src) == at)
        .unwrap_or_else(|| panic!("no token `{at}`"));
    tokens[i..].to_vec()
}

fn text_of(src: &str, node: &Node) -> String {
    let (start, end) = node.range();
    src[start as usize..end as usize].to_owned()
}

#[test]
fn a_head_is_a_plugin_block_only_where_the_file_imports_it() {
    let call = parse_str("let x = pairs { a = 1 }\n");
    assert!(call.diagnostics.is_empty(), "{:?}", call.diagnostics);
    assert!(find(&call.tree, K::PluginExpr).is_none());
    assert!(find(&call.tree, K::RecordExpr).is_some());

    let src = format!("{IMPORT}let x = pairs {{ a = {{ b = 1 }}; c = 2 }}\nlet y = pairs\n");
    let parsed = parse_str(&src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let import = find(&parsed.tree, K::ImportDecl).expect("an import");
    let items: Vec<String> = find(import, K::Exposing)
        .expect("exposed items")
        .nodes()
        .map(|item| text_of(&src, item))
        .collect();
    assert_eq!(items, ["Pair", "plugin pairs"]);
    assert!(
        find(import, K::Exposing).is_some_and(|e| e.tokens().iter().any(|t| t.kind == K::PluginKw))
    );
    let block = find(&parsed.tree, K::PluginExpr).expect("a plugin block");
    assert_eq!(text_of(&src, block), "pairs { a = { b = 1 }; c = 2 }");
    // the body is kept as tokens, braces balanced
    let body = block.nodes().find(|n| n.kind == K::Block).expect("a body");
    assert!(body.nodes().next().is_none());
    assert_eq!(text_of(&src, body), "a = { b = 1 }; c = 2");
    // without `{` the head is a name
    let names = parsed.tree.nodes().filter(|n| n.kind == K::LetDecl).nth(1);
    assert!(find(names.expect("let y"), K::NameExpr).is_some());
}

#[test]
fn an_unclosed_block_is_reported() {
    let src = format!("{IMPORT}let x = pairs {{ a = 1\n");
    let codes: Vec<String> = parse_str(&src)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect();
    assert!(codes.iter().any(|c| c.ends_with("S008")), "{codes:?}");
}

#[test]
fn a_hole_is_the_longest_expression_where_it_stands() {
    let src = format!("{IMPORT}let x = pairs {{ a = f 1 2; b = 3 }}\n");
    let (_, tokens) = body(&src);
    let file = FileId::default();
    let hole = parse_expr_at(&src, file, &from(&src, &tokens, "f"), &[]).expect("an expression");
    assert!(hole.diagnostics.is_empty(), "{:?}", hole.diagnostics);
    assert_eq!(text_of(&src, &hole.tree), "f 1 2");
    assert_eq!(&src[..hole.end as usize].rsplit(' ').next(), &Some("2"));

    let src = format!("{IMPORT}let x =\n  pairs {{\n    a = f 1\n      2\n    b = 3\n  }}\n");
    let (_, tokens) = body(&src);
    let hole = parse_expr_at(&src, file, &from(&src, &tokens, "f"), &[]).expect("an expression");
    assert!(hole.diagnostics.is_empty(), "{:?}", hole.diagnostics);
    // deeper lines continue it, the next field's line ends it
    assert_eq!(text_of(&src, &hole.tree), "f 1\n      2");

    // a hole on a line of its own is a block of its own
    let src = format!("{IMPORT}let x = pairs {{\n  f 1\n    2\n  g\n}}\n");
    let (_, tokens) = body(&src);
    let hole = parse_expr_at(&src, file, &from(&src, &tokens, "f"), &[]).expect("an expression");
    assert_eq!(text_of(&src, &hole.tree), "f 1\n    2");
}

#[test]
fn a_type_hole_ends_where_the_type_does() {
    let src = format!("{IMPORT}let x = pairs {{ b : List Int = 2 }}\n");
    let (_, tokens) = body(&src);
    let hole =
        parse_type_at(&src, FileId::default(), &from(&src, &tokens, "List"), &[]).expect("a type");
    assert!(hole.diagnostics.is_empty(), "{:?}", hole.diagnostics);
    assert_eq!(text_of(&src, &hole.tree), "List Int");
}

#[test]
fn no_hole_starts_at_punctuation() {
    let src = format!("{IMPORT}let x = pairs {{ a = 1; b = 2 }}\n");
    let (_, tokens) = body(&src);
    let hole = parse_expr_at(&src, FileId::default(), &from(&src, &tokens, ";"), &[]);
    assert!(
        hole.as_ref().is_none_or(|h| !h.diagnostics.is_empty()),
        "{:?}",
        hole.map(|h| h.tree)
    );
}

#[test]
fn a_head_with_arguments_and_no_body_is_a_block() {
    let src = format!(
        "{IMPORT}let x = f (pairs Shape) (pairs App.Api.place)\nlet y = pairs \"a\" 1 (2)\nlet z = pairs\nlet w = pairs + 1\n"
    );
    let parsed = parse_str(&src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let blocks: Vec<String> = (parsed.tree.nodes())
        .flat_map(|d| {
            let mut found = Vec::new();
            collect(d, K::PluginExpr, &mut found);
            found
        })
        .map(|b| text_of(&src, b))
        .collect();
    assert_eq!(
        blocks,
        ["pairs Shape", "pairs App.Api.place", "pairs \"a\" 1 (2)"]
    );
    // the raw text of a block is a string argument, which lexes whatever it holds
    let raw = format!(
        "{IMPORT}let ddl = pairs r#\"\n  create table t (name text default 'a') -- why\n\"#\n"
    );
    let parsed = parse_str(&raw);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let block = find(&parsed.tree, K::PluginExpr).expect("a plugin block");
    assert!(text_of(&raw, block).ends_with("-- why\n\"#"));
    // the compiler's own heads need their body, so a value `cli` stays one
    let value = parse_str("let x = cli y\n");
    assert!(find(&value.tree, K::PluginExpr).is_none());
}

/// Every node of `kind` under `node`.
fn collect<'n>(node: &'n Node, kind: SyntaxKind, out: &mut Vec<&'n Node>) {
    if node.kind == kind {
        out.push(node);
    }
    for child in node.nodes() {
        collect(child, kind, out);
    }
}

#[test]
fn a_head_of_the_compilers_own_plugins_is_a_block_without_its_import() {
    // so the compiler can name the import it needs
    let parsed = parse_str("let c = cli \"x\" { handle f }\nlet d = cli \"x\"\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let block = find(&parsed.tree, K::PluginExpr).expect("a plugin block");
    assert_eq!(
        text_of("let c = cli \"x\" { handle f }", block),
        "cli \"x\" { handle f }"
    );
    let names = parsed.tree.nodes().filter(|n| n.kind == K::LetDecl).nth(1);
    assert!(find(names.expect("let d"), K::PluginExpr).is_none());
}

#[test]
fn a_hole_is_a_pattern_or_a_declaration_where_it_stands() {
    let src = format!("{IMPORT}pairs let f (n : Int) x =\n  n + x\n\nlet after = 1\n");
    let parsed = parse_str(&src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let block = find(&parsed.tree, K::PluginDecl).expect("a declaration");
    let tokens = block.tokens();
    let file = FileId::default();
    let pat = parse_pat_at(&src, file, &from(&src, &tokens, "("), &[]).expect("a pattern");
    assert!(pat.diagnostics.is_empty(), "{:?}", pat.diagnostics);
    assert_eq!(text_of(&src, &pat.tree), "(n : Int)");
    let decl = parse_decl_at(&src, file, &from(&src, &tokens, "let"), &[]).expect("a declaration");
    assert!(decl.diagnostics.is_empty(), "{:?}", decl.diagnostics);
    assert_eq!(decl.tree.kind, K::LetDecl);
    assert_eq!(text_of(&src, &decl.tree), "let f (n : Int) x =\n  n + x");
}

/// The shape of a node: its kind and those of its nodes, as `App(Name, Children(…))`.
fn shape(node: &Node) -> String {
    let inner: Vec<String> = node.nodes().map(shape).collect();
    let kind = format!("{:?}", node.kind);
    let kind = kind.trim_end_matches("Expr");
    if inner.is_empty() {
        kind.to_owned()
    } else {
        format!("{kind}({})", inner.join(", "))
    }
}

#[test]
fn the_lines_under_a_call_are_its_children_where_a_block_asks_for_them() {
    let src = format!(
        "{IMPORT}let v = pairs {{\n  div props\n    heading (text \"a\")\n    paragraph\n      text \"b\"\n      text \"c\"\n    spacer\n  last\n}}\n"
    );
    let (_, tokens) = body(&src);
    let file = FileId::default();
    let lines = parse_lines_at(&src, file, &from(&src, &tokens, "div"), &[]).expect("a body");
    assert!(lines.diagnostics.is_empty(), "{:?}", lines.diagnostics);
    // two statements: the call with its children, nested by their indent, and `last`
    assert_eq!(
        shape(&lines.tree),
        "Block(App(Name, Name, Children(App(Name, Paren(App(Name, Literal))), \
         App(Name, Children(App(Name, Literal), App(Name, Literal))), Name)), Name)"
    );
    // read as a body, the same lines are the arguments of the call, one by one
    let body = parse_body_at(&src, file, &from(&src, &tokens, "div"), &[]).expect("a body");
    assert!(
        !shape(&body.tree).contains("Children"),
        "{}",
        shape(&body.tree)
    );
}

#[test]
fn a_line_that_goes_on_with_an_expression_is_no_child() {
    // inside brackets, after an operator and at a branch of an `if`
    let src = format!(
        "{IMPORT}let v = pairs {{\n  text (join\n    names)\n  first\n    |> second\n  if ready then\n    shown\n  else\n    hidden\n}}\n"
    );
    let (_, tokens) = body(&src);
    let lines =
        parse_lines_at(&src, FileId::default(), &from(&src, &tokens, "text"), &[]).expect("a body");
    assert!(lines.diagnostics.is_empty(), "{:?}", lines.diagnostics);
    assert!(
        !shape(&lines.tree).contains("Children"),
        "{}",
        shape(&lines.tree)
    );
}

#[test]
fn a_suite_and_a_test_are_blocks_only_where_a_top_level_declaration_starts() {
    let cat = test_catalog();
    let sexp_with = |src: &str| {
        let parsed = parse_with(src, &cat);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        super::tests::sexp_of(&parsed.tree, src)
    };
    assert_eq!(
        sexp_with("suite \"Users\" { test \"a\" {} }\ntest.skip \"b\" {}\n"),
        "(PluginDecl suite \"Users\" { (Block test \"a\" { }) })\n\
         (PluginDecl test . skip \"b\" { (Block ) })"
    );
    // the rows of a `test.each` are a bracketed argument, kept as tokens
    assert_eq!(
        sexp_with("test.each [(1, 2)] \"adds {0}\" (a, b) {}\n"),
        "(PluginDecl test . each [ (PluginArgs ( 1 , 2 )) ] \"adds {0}\" ( (PluginArgs a , b) ) { (Block ) })"
    );
    // a signature of a value of that name stays one, and the words stay names
    // in expressions, where `Test.test` and a local `test` are called
    let expr_src = "test : String -> Int\nlet test name = 1\nlet a = test \"x\"\nlet b = Test.test \"x\" (fun () -> Test.Pass)\nlet c = suite \"y\" { x = 1 }\n";
    let parsed = parse_with(expr_src, &cat);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(sexp_with("let c = suite \"y\" { x = 1 }\n").contains("RecordExpr"));
    // without a catalog, `suite` is not treated as an implicit plugin head
    assert_eq!(
        errors("let c = suite \"y\" { x = 1 }\n"),
        Vec::<String>::new()
    );
    let _ = sexp;
}

#[test]
fn external_head_rules_drive_top_bare_declarations_named_and_brackets() {
    let cat = HeadCatalog::new(vec![
        HeadRule {
            head: "spec".into(),
            module: "Spec".into(),
            implicit: false,
            bare: false,
            declarations: Vec::new(),
            named: false,
            top: true,
            bracket_args: true,
        },
        HeadRule {
            head: "route".into(),
            module: "Router".into(),
            implicit: false,
            bare: false,
            declarations: vec!["group".into()],
            named: false,
            top: false,
            bracket_args: false,
        },
        HeadRule {
            head: "widget".into(),
            module: "Widgets".into(),
            implicit: false,
            bare: false,
            declarations: Vec::new(),
            named: true,
            top: false,
            bracket_args: false,
        },
    ]);
    let src = "import Spec exposing (plugin spec)\n\
               import Router exposing (plugin route)\n\
               import Widgets exposing (plugin widget)\n\
               spec [1, 2] \"case\" { ok }\n\
               route group api = 1\n\
               widget counter x = x\n\
               let a = spec \"call\" { x = 1 }\n\
               let b = route \"/home\" { ok }\n\
               let c = route plain_call\n";
    let parsed = parse_with(src, &cat);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let decls: Vec<String> = parsed
        .tree
        .nodes()
        .filter(|n| n.kind == K::PluginDecl)
        .map(|n| text_of(src, n))
        .collect();
    assert_eq!(
        decls,
        [
            "spec [1, 2] \"case\" { ok }",
            "route group api = 1",
            "widget counter x = x",
        ]
    );
    let mut exprs = Vec::new();
    collect(&parsed.tree, K::PluginExpr, &mut exprs);
    let exprs: Vec<String> = exprs.into_iter().map(|n| text_of(src, n)).collect();
    // `spec` is top-only so `let a = spec ...` is NOT a PluginExpr;
    // `route` has bare=false so `route plain_call` is NOT a PluginExpr, only `route "/home" { ok }` is!
    assert_eq!(exprs, ["route \"/home\" { ok }"]);
}

#[test]
fn a_component_is_a_declaration_of_std_s_plugin() {
    let parsed = parse_str("component label (p : P) = text p.name\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        super::tests::sexp_of(&parsed.tree, "component label (p : P) = text p.name\n"),
        "(PluginDecl component label ( (PluginArgs p : P) ) (PluginRest = text p . name))"
    );
    let src = "component counter (p : P) =\n  let n = 1\n  show n\nlet x = 1\n";
    let parsed2 = parse_str(src);
    assert!(parsed2.diagnostics.is_empty(), "{:?}", parsed2.diagnostics);
    assert_eq!(
        super::tests::sexp_of(&parsed2.tree, src),
        "(PluginDecl component counter ( (PluginArgs p : P) ) (PluginRest = let n = 1 show n))\n\
         (LetDecl let (VarPat x) = (Block (Literal 1)))"
    );
    assert!(
        parse_str("let render component setup = component setup\n")
            .diagnostics
            .is_empty()
    );
}

#[test]
fn a_server_fn_and_server_name_are_blocks_of_std_without_an_import() {
    let s1 = "server fn place order = order\n";
    let p1 = parse_str(s1);
    assert!(p1.diagnostics.is_empty(), "{:?}", p1.diagnostics);
    assert_eq!(
        super::tests::sexp_of(&p1.tree, s1),
        "(PluginDecl server fn place order (PluginRest = order))"
    );
    let s2 = "let x = [server place, server Api.place]\n";
    let p2 = parse_str(s2);
    assert!(p2.diagnostics.is_empty(), "{:?}", p2.diagnostics);
    assert_eq!(
        super::tests::sexp_of(&p2.tree, s2),
        "(LetDecl let (VarPat x) = (Block (ListExpr [ (PluginExpr server place) , \
         (PluginExpr server Api . place) ])))"
    );
    let src = "let f server fn = server\nlet g server = (server)\nlet h = call server double 21\n";
    assert!(parse_str(src).diagnostics.is_empty());
}
