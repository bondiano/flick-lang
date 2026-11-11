//! Finding nodes and tokens of a concrete tree by position.

use std::collections::HashMap;

use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// The nodes from `root` down to the innermost one containing `offset`.
pub fn path_to(root: &Node, offset: u32) -> Vec<&Node> {
    let mut out = vec![root];
    let mut node = root;
    while let Some(child) = node
        .nodes()
        .find(|n| n.range().0 <= offset && offset <= n.range().1)
    {
        out.push(child);
        node = child;
    }
    out
}

/// The node of exactly `range`.
pub fn find(root: &Node, range: (u32, u32)) -> Option<&Node> {
    if root.range() == range {
        return Some(root);
    }
    root.nodes()
        .filter(|n| n.range().0 <= range.0 && range.1 <= n.range().1)
        .find_map(|n| find(n, range))
}

/// Each node under `root` by its range, as [`find`] gives it: the outermost
/// of those that share one. For a caller that looks many up, since `find`
/// walks the declarations of the file on every call.
pub fn by_range(root: &Node) -> HashMap<(u32, u32), &Node> {
    fn walk<'t>(node: &'t Node, out: &mut HashMap<(u32, u32), &'t Node>) {
        out.entry(node.range()).or_insert(node);
        for child in node.nodes() {
            walk(child, out);
        }
    }
    let mut out = HashMap::new();
    walk(root, &mut out);
    out
}

/// The last name of a name node, through same-range wrappers such as a row.
pub fn last_name(node: &Node) -> Option<Token> {
    node.child_tokens()
        .filter(|t| matches!(t.kind, K::Ident | K::Con))
        .last()
        .or_else(|| node.nodes().filter_map(last_name).last())
}

/// The name token touching `offset` among `node`'s own tokens.
pub fn name_token_at(node: &Node, offset: u32) -> Option<Token> {
    node.child_tokens()
        .find(|t| matches!(t.kind, K::Ident | K::Con) && t.start <= offset && offset <= t.end)
}

/// The smallest entry of `ranges` containing `offset`.
pub fn innermost<'a, V>(
    entries: impl Iterator<Item = (&'a (u32, u32), V)>,
    offset: u32,
) -> Option<((u32, u32), V)> {
    entries
        .filter(|((s, e), _)| *s <= offset && offset <= *e)
        .min_by_key(|((s, e), _)| e - s)
        .map(|(r, v)| (*r, v))
}

/// The text of the first token of `kind` among `node`'s own tokens.
pub fn name_of<'t>(node: &Node, kind: K, text: &'t str) -> Option<&'t str> {
    node.child_tokens()
        .find(|t| t.kind == kind)
        .map(|t| t.text(text))
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{by_range, find};
    use crate::source::FileId;
    use crate::syntax;
    use crate::syntax::cst::Node;

    fn each<'t>(node: &'t Node, out: &mut Vec<&'t Node>) {
        out.push(node);
        node.nodes().for_each(|child| each(child, out));
    }

    #[test]
    fn the_nodes_by_range_are_the_ones_found_one_at_a_time() {
        let text = "module M exposing (f)\n\nf : Int -> {x : Int | r} -> Int\nlet f n record =\n  let (a, b) = (n, record.x)\n  if a > b then a else (b)\n";
        let tree = syntax::parse(text, FileId::default()).tree;
        let indexed = by_range(&tree);
        let mut all = Vec::new();
        each(&tree, &mut all);
        assert!(all.len() > 20);
        for node in all {
            let found = find(&tree, node.range()).expect("a node of its own range");
            assert!(ptr::eq(indexed[&node.range()], found), "{:?}", node.kind);
        }
    }
}
