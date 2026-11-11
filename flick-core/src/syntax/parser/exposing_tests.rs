//! The items of an `exposing` list.

use super::*;
use crate::source::SourceMap;

fn parse_str(src: &str) -> Parse {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src);
    parse(src, file)
}

#[test]
fn an_export_list_names_a_definition_that_spells_a_keyword() {
    let parsed = parse_str("module M exposing (none, with)");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let header = parsed.tree.nodes().next().expect("a header");
    let exposing = header
        .nodes()
        .find(|n| n.kind == K::Exposing)
        .expect("a list");
    let items: Vec<_> = exposing
        .nodes()
        .flat_map(Node::tokens)
        .map(|t| t.kind)
        .collect();
    assert_eq!(items, [K::Ident, K::Ident]);
    // a keyword that goes on is not taken for a name
    let wrong = parse_str("module M exposing (a, let x = 1");
    let found: Vec<_> = wrong.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(found, [codes::S007]);
}
