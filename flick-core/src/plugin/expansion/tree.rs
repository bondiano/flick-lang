//! Tree splicing for expanded blocks and in-place hole parsing.

use std::iter::Peekable;
use std::mem;

use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

/// The code of each block and whether the block stays before it, by range.
pub(super) type Spliced = Vec<((u32, u32), Vec<Node>, bool)>;

/// Replaces each block under `node` by its code from `spliced`, by range:
/// an expression by its tree, a declaration of the module by its
/// declarations; a type declaration a derive gives code for stays, its
/// code after it.
pub(super) fn replace_blocks(node: &mut Node, spliced: &mut Spliced) {
    let mut children = Vec::with_capacity(node.children.len());
    for child in mem::take(&mut node.children) {
        let Element::Node(mut n) = child else {
            children.push(child);
            continue;
        };
        if matches!(
            n.kind,
            K::PluginExpr
                | K::PluginDecl
                | K::TypeDecl
                | K::ErrorDecl
                | K::DerivingDecl
                | K::Signature
        ) {
            let range = n.range();
            let (mut code, mut found, mut keep) = (Vec::new(), false, false);
            while let Some(i) = spliced.iter().position(|(r, ..)| *r == range) {
                let (_, nodes, kept) = spliced.remove(i);
                (found, keep) = (true, keep || kept);
                code.extend(nodes);
            }
            if found {
                if keep {
                    children.push(Element::Node(n));
                }
                // the user's code in its code has blocks of its own
                for mut code in code {
                    replace_blocks(&mut code, spliced);
                    children.push(Element::Node(code));
                }
                continue;
            }
        } else {
            replace_blocks(&mut n, spliced);
        }
        children.push(Element::Node(n));
    }
    node.children = children;
}

/// Replaces the tokens of the user's code inside each block under `node` by
/// that code parsed, from `holes` (a block's range and a tree of its code).
pub(super) fn parse_holes(node: &mut Node, holes: &[((u32, u32), Node)]) {
    for child in &mut node.children {
        let Element::Node(n) = child else {
            continue;
        };
        if !matches!(n.kind, K::PluginExpr | K::PluginDecl) {
            parse_holes(n, holes);
            continue;
        }
        let range = n.range();
        let mut trees: Vec<&Node> = holes
            .iter()
            .filter(|(r, _)| *r == range)
            .map(|(_, t)| t)
            .collect();
        trees.sort_by_key(|t| t.range());
        trees.dedup_by_key(|t| t.range());
        let mut next = trees.into_iter().peekable();
        n.children = in_place(mem::take(&mut n.children), &mut next);
        // the blocks inside that code
        parse_holes(n, holes);
    }
}

/// `elements` of a block with the tokens of each tree `next` gives replaced
/// by the tree: its string arguments, and inside `( … )` and the body.
fn in_place<'t>(
    elements: Vec<Element>,
    next: &mut Peekable<impl Iterator<Item = &'t Node>>,
) -> Vec<Element> {
    let mut children = Vec::new();
    for element in elements {
        let t = match element {
            Element::Token(t) => t,
            Element::Node(mut raw)
                if matches!(raw.kind, K::PluginArgs | K::Block | K::PluginRest) =>
            {
                raw.children = in_place(mem::take(&mut raw.children), next);
                children.push(Element::Node(raw));
                continue;
            }
            other @ Element::Node(_) => {
                children.push(other);
                continue;
            }
        };
        // a tree that took no token of the block is none of its own
        while next.peek().is_some_and(|tree| tree.range().1 <= t.start) {
            next.next();
        }
        match next.peek() {
            Some(tree) if tree.range().0 <= t.start && t.end <= tree.range().1 => {
                if tree.range().1 == t.end {
                    children.push(Element::Node((*tree).clone()));
                    next.next();
                }
            }
            _ => children.push(Element::Token(t)),
        }
    }
    children
}
