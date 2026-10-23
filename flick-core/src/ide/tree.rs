//! Finding nodes and tokens of a concrete tree by position.

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
