use super::parse_str;
use crate::fmt;
use crate::source::FileId;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

const IMPORT: &str = "import Pairs exposing (Pair, plugin pairs)\n\n";

/// The kinds of the nodes from the root down to the first `kind`.
fn find(node: &Node, kind: K) -> Option<&Node> {
    if node.kind == kind {
        return Some(node);
    }
    node.nodes().find_map(|n| find(n, kind))
}

fn text_of(src: &str, node: &Node) -> String {
    let (start, end) = node.range();
    src[start as usize..end as usize].to_owned()
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
