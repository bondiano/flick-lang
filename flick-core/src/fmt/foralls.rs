//! Remove an outer `forall` only where implicit signature variables have the
//! same scope. The checker treats a complete explicit `forall` differently
//! for polymorphic recursion, so recursive bindings are never rewritten.

use std::collections::HashSet;

use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_type;

pub(super) fn drop_redundant_foralls(tree: &mut Node, text: &str) {
    visit(tree, text, &HashSet::new(), true);
}

fn visit(node: &mut Node, text: &str, outer: &HashSet<String>, scope_known: bool) {
    // Instance methods can generalize their own signature variables, but
    // those variables must not shadow the instance head or its constraints.
    if node.kind == K::InstanceDecl {
        let mut scoped = outer.clone();
        for child in &mut node.children {
            if let Element::Node(child) = child {
                if child.kind == K::Members {
                    visit(child, text, &scoped, scope_known);
                } else if child.kind == K::Context || is_type(child.kind) {
                    scoped.extend(type_names(child, text));
                }
            }
        }
        return;
    }
    // Class and service binders, and recursive groups, need their written
    // quantifiers to preserve the checker's scoping and recursion rules.
    if matches!(node.kind, K::ClassDecl | K::ServiceDecl | K::RecGroup) {
        return;
    }
    if matches!(node.kind, K::SourceFile | K::Block | K::Members) {
        let positions: Vec<usize> = node
            .children
            .iter()
            .enumerate()
            .filter_map(|(i, child)| matches!(child, Element::Node(_)).then_some(i))
            .collect();
        for pair in positions.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            let (before, after) = node.children.split_at_mut(right);
            let (Element::Node(sig), Element::Node(decl)) = (&mut before[left], &after[0]) else {
                continue;
            };
            if sig.kind != K::Signature
                || decl.kind != K::LetDecl
                || decl.has_token(K::RecKw)
                || !same_name(sig, decl, text)
            {
                continue;
            }
            if scope_known {
                shorten(sig, text, outer);
            }
        }
        // Each let body has the variables of its own signature in scope.
        for (at, position) in positions.iter().enumerate() {
            let sig_vars = at.checked_sub(1)
                .and_then(|i| node.children.get(positions[i]))
                .and_then(|child| match child { Element::Node(n) if n.kind == K::Signature => Some(n), _ => None })
                .filter(|sig| node.children.get(*position).is_some_and(|child| {
                    matches!(child, Element::Node(decl) if decl.kind == K::LetDecl && same_name(sig, decl, text))
                }))
                .and_then(signature_type)
                .map(|ty| type_names(ty, text));
            let Some(Element::Node(child)) = node.children.get_mut(*position) else {
                continue;
            };
            let mut scoped = outer.clone();
            if let Some(names) = &sig_vars {
                scoped.extend(names.iter().cloned());
            }
            let known = scope_known && (child.kind != K::LetDecl || sig_vars.is_some());
            visit(child, text, &scoped, known);
        }
        return;
    }
    for child in &mut node.children {
        if let Element::Node(child) = child {
            visit(child, text, outer, scope_known);
        }
    }
}

fn same_name(sig: &Node, decl: &Node, text: &str) -> bool {
    let name = sig
        .child_tokens()
        .find(|t| t.kind == K::Ident)
        .map(|t| t.text(text));
    name.is_some()
        && decl
            .nodes()
            .find(|n| n.kind == K::VarPat)
            .and_then(|n| n.child_tokens().find(|t| t.kind == K::Ident))
            .map(|t| t.text(text))
            == name
}

fn signature_type(sig: &Node) -> Option<&Node> {
    sig.nodes().find(|n| is_type(n.kind))
}

fn type_names(node: &Node, text: &str) -> HashSet<String> {
    node.tokens()
        .into_iter()
        .filter(|t| t.kind == K::Ident)
        .map(|t| t.text(text).to_owned())
        .collect()
}

fn shorten(sig: &mut Node, text: &str, outer: &HashSet<String>) {
    let Some(Element::Node(ty)) = sig
        .children
        .iter_mut()
        .find(|c| matches!(c, Element::Node(n) if is_type(n.kind)))
    else {
        return;
    };
    if ty.kind != K::ForallType {
        return;
    }
    let vars: Vec<String> = ty
        .child_tokens()
        .filter(|t| t.kind == K::Ident)
        .map(|t| t.text(text).to_owned())
        .collect();
    let Some(body) = ty.nodes().next() else {
        return;
    };
    let distinct: HashSet<&str> = vars.iter().map(String::as_str).collect();
    if vars.is_empty()
        || distinct.len() != vars.len()
        || body.kind == K::ForallType
        || vars.iter().any(|v| outer.contains(v))
    {
        return;
    }
    // Replacing the wrapper must never discard a comment attached to it.
    if ty.tokens().iter().any(|t| t.kind.is_comment()) {
        return;
    }
    let mut used = HashSet::new();
    used_names(body, text, &mut used);
    if vars.iter().any(|v| !used.contains(v)) {
        return;
    }
    *ty = body.clone();
}

fn used_names(node: &Node, text: &str, used: &mut HashSet<String>) {
    if node.kind == K::ForallType {
        return;
    }
    if node.kind == K::NameType {
        used.extend(
            node.child_tokens()
                .filter(|t| t.kind == K::Ident)
                .map(|t| t.text(text).to_owned()),
        );
    }
    for child in node.nodes() {
        used_names(child, text, used);
    }
}
