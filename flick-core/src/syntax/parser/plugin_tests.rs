use super::*;
use crate::fmt;
use crate::source::SourceMap;
use crate::syntax::cst::{Element, Token};

const IMPORT: &str = "import plugin \"pairs\" exposing (pairs)\n\n";

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
    let import = find(&parsed.tree, K::PluginImport).expect("a plugin import");
    assert!(import.has_token(K::PluginKw));
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
    let hole = parse_expr_at(&src, file, &from(&src, &tokens, "f")).expect("an expression");
    assert!(hole.diagnostics.is_empty(), "{:?}", hole.diagnostics);
    assert_eq!(text_of(&src, &hole.tree), "f 1 2");
    assert_eq!(&src[..hole.end as usize].rsplit(' ').next(), &Some("2"));

    let src = format!("{IMPORT}let x =\n  pairs {{\n    a = f 1\n      2\n    b = 3\n  }}\n");
    let (_, tokens) = body(&src);
    let hole = parse_expr_at(&src, file, &from(&src, &tokens, "f")).expect("an expression");
    assert!(hole.diagnostics.is_empty(), "{:?}", hole.diagnostics);
    // deeper lines continue it, the next field's line ends it
    assert_eq!(text_of(&src, &hole.tree), "f 1\n      2");

    // a hole on a line of its own is a block of its own
    let src = format!("{IMPORT}let x = pairs {{\n  f 1\n    2\n  g\n}}\n");
    let (_, tokens) = body(&src);
    let hole = parse_expr_at(&src, file, &from(&src, &tokens, "f")).expect("an expression");
    assert_eq!(text_of(&src, &hole.tree), "f 1\n    2");
}

#[test]
fn a_type_hole_ends_where_the_type_does() {
    let src = format!("{IMPORT}let x = pairs {{ b : List Int = 2 }}\n");
    let (_, tokens) = body(&src);
    let hole =
        parse_type_at(&src, FileId::default(), &from(&src, &tokens, "List")).expect("a type");
    assert!(hole.diagnostics.is_empty(), "{:?}", hole.diagnostics);
    assert_eq!(text_of(&src, &hole.tree), "List Int");
}

#[test]
fn no_hole_starts_at_punctuation() {
    let src = format!("{IMPORT}let x = pairs {{ a = 1; b = 2 }}\n");
    let (_, tokens) = body(&src);
    let hole = parse_expr_at(&src, FileId::default(), &from(&src, &tokens, ";"));
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
    let parsed = parse_str(&src);
    let imports = parsed
        .tree
        .children
        .iter()
        .filter(|c| matches!(c, Element::Node(n) if n.kind == K::PluginImport))
        .count();
    assert_eq!(imports, 1);
}
