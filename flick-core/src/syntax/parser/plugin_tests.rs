use super::*;
use crate::fmt;
use crate::source::SourceMap;
use crate::syntax::cst::{Element, Token};

const IMPORT: &str = "import Pairs exposing (Pair, plugin pairs)\n\n";

fn parse_str(src: &str) -> Parse {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    parse(src, file)
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
fn the_formatter_keeps_plugin_imports_and_blocks() {
    let src = format!("{IMPORT}x : Int\nlet x = pairs {{ a = 1;   b : Int = 2 }}\n");
    let formatted = fmt::format(&src, FileId::default()).expect("formats");
    assert_eq!(formatted, src);
    // `plugin` stays a name wherever it does not start an exposed head
    let named = parse_str("let plugin = 1\nlet x = plugin\n");
    assert!(named.diagnostics.is_empty(), "{:?}", named.diagnostics);
    let parsed = parse_str(&src);
    let imports = parsed
        .tree
        .children
        .iter()
        .filter(|c| matches!(c, Element::Node(n) if n.kind == K::ImportDecl))
        .count();
    assert_eq!(imports, 1);
}

#[test]
fn a_head_takes_strings_and_parenthesized_arguments_before_its_body() {
    let src = format!("{IMPORT}let x = pairs \"a\" (f (g 1)) \"b\" {{ c = 1 }}\n");
    let parsed = parse_str(&src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let block = find(&parsed.tree, K::PluginExpr).expect("a plugin block");
    assert_eq!(
        text_of(&src, block),
        "pairs \"a\" (f (g 1)) \"b\" { c = 1 }"
    );
    let args = block
        .nodes()
        .find(|n| n.kind == K::PluginArgs)
        .expect("arguments");
    assert_eq!(text_of(&src, args), "f (g 1)");
    let body = block.nodes().find(|n| n.kind == K::Block).expect("a body");
    assert_eq!(text_of(&src, body), "c = 1");
    let formatted = fmt::format(&src, FileId::default()).expect("formats");
    assert_eq!(formatted, src);

    // a head is no value: a name argument is one too
    let named = parse_str(&format!("{IMPORT}let x = pairs a {{ b = 1 }}\n"));
    assert!(named.diagnostics.is_empty(), "{:?}", named.diagnostics);
    let block = find(&named.tree, K::PluginExpr).expect("a plugin block");
    assert!(block.nodes().any(|n| n.kind == K::Block));
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
fn a_head_starts_a_declaration_of_words_a_body_or_the_rest() {
    let src = format!(
        "{IMPORT}pairs table Name \"t\" {{\n  x : Float\n}}\n\npairs fn ping (n : Int) =\n  let m = n + 1\n  m\n\npairs migration 1 \"init\" r#\"create table t ();\"#\n\npairs : Float\nlet after = 1\n"
    );
    let parsed = parse_str(&src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let decls: Vec<&Node> = (parsed.tree.nodes())
        .filter(|n| n.kind == K::PluginDecl)
        .collect();
    let texts: Vec<String> = decls.iter().map(|d| text_of(&src, d)).collect();
    assert_eq!(
        texts,
        [
            "pairs table Name \"t\" {\n  x : Float\n}",
            "pairs fn ping (n : Int) =\n  let m = n + 1\n  m",
            "pairs migration 1 \"init\" r#\"create table t ();\"#",
        ]
    );
    assert!(decls[0].nodes().any(|n| n.kind == K::Block));
    let rest = decls[1]
        .nodes()
        .find(|n| n.kind == K::PluginRest)
        .expect("the rest");
    assert_eq!(text_of(&src, rest), "=\n  let m = n + 1\n  m");
    // a signature of the head's name is a signature, and what follows parses
    assert!(parsed.tree.nodes().any(|n| n.kind == K::Signature));
    assert!(parsed.tree.nodes().any(|n| n.kind == K::LetDecl));
    let formatted = fmt::format(&src, FileId::default()).expect("formats");
    assert_eq!(formatted, src);
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
