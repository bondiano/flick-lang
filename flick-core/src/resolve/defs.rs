//! Top-level definitions of one module, duplicate definitions and cycles of
//! transparent type aliases.

use std::collections::{HashMap, VecDeque};

use super::{Decl, Namespace};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug, Clone)]
pub(super) struct Def {
    pub namespace: Namespace,
    pub name: String,
    pub span: Span,
    /// Type of a constructor, class of a method, `kind` of a kind member.
    pub parent: Option<String>,
    pub decl: Decl,
    /// A signature; pairing with its definition is checked later.
    pub signature: bool,
}

pub(super) fn collect(tree: &Node, text: &str, file: FileId) -> Vec<Def> {
    let mut defs = Vec::new();
    for decl in tree.nodes() {
        declaration(decl, text, file, &mut defs);
    }
    defs
}

fn declaration(decl: &Node, text: &str, file: FileId, defs: &mut Vec<Def>) {
    let decl_node = decl;
    let def = |namespace, token: Token, parent: Option<&str>, decl| Def {
        namespace,
        name: token.text(text).to_owned(),
        span: Span::new(file, token.start, token.end),
        parent: parent.map(str::to_owned),
        decl,
        signature: decl_node.kind == K::Signature,
    };
    let first = |kind: K| decl.child_tokens().find(|t| t.kind == kind);
    match decl.kind {
        K::Signature => {
            defs.extend(first(K::Ident).map(|t| def(Namespace::Value, t, None, Decl::Let)));
        }
        K::ForeignDecl => {
            // `foreign <mode> <name> …`
            let name = decl.child_tokens().filter(|t| t.kind == K::Ident).nth(1);
            defs.extend(name.map(|t| def(Namespace::Value, t, None, Decl::Foreign)));
        }
        K::LetDecl => {
            let mut names = Vec::new();
            match decl.nodes().next() {
                Some(name) if name.kind == K::VarPat => {
                    names.extend(name.child_tokens().find(|t| t.kind == K::Ident));
                }
                Some(pattern) => binders(pattern, &mut names),
                None => {}
            }
            defs.extend(
                names
                    .into_iter()
                    .map(|t| def(Namespace::Value, t, None, Decl::Let)),
            );
        }
        K::RecGroup => {
            for members in decl.nodes().filter(|n| n.kind == K::Members) {
                for member in members.nodes() {
                    declaration(member, text, file, defs);
                }
            }
        }
        K::TypeDecl | K::KindDecl | K::ErrorDecl | K::ServiceDecl | K::ClassDecl => {
            let Some(name) = first(K::Con) else { return };
            let parent = name.text(text);
            let variants: Vec<Token> = decl
                .nodes()
                .filter(|n| n.kind == K::Variant)
                .filter_map(|v| v.child_tokens().find(|t| t.kind == K::Con))
                .collect();
            let form = match decl.kind {
                K::TypeDecl if decl.has_token(K::OpaqueKw) => Decl::Opaque,
                K::TypeDecl if decl.has_token(K::RefinedKw) => Decl::Refined,
                K::TypeDecl if !variants.is_empty() => Decl::Adt,
                K::TypeDecl if decl.has_token(K::Eq) => Decl::Alias,
                K::TypeDecl => Decl::Abstract,
                K::KindDecl => Decl::Kind,
                K::ErrorDecl => Decl::Error,
                K::ServiceDecl => Decl::Service,
                _ => Decl::Class,
            };
            defs.push(def(Namespace::Type, name, None, form));
            // kind members are type-level names: `Js.Nullable NonNull a`
            let (member_space, member) = match decl.kind {
                K::KindDecl => (Namespace::Type, Decl::KindMember),
                _ => (Namespace::Constructor, Decl::Constructor),
            };
            // the constructor of an error is the error's own name, with or without fields
            if decl.kind == K::ErrorDecl {
                defs.push(def(
                    Namespace::Constructor,
                    name,
                    Some(parent),
                    Decl::Constructor,
                ));
            } else {
                for variant in variants {
                    defs.push(def(member_space, variant, Some(parent), member));
                }
            }
            let methods = decl
                .nodes()
                .filter(|n| n.kind == K::Members)
                .flat_map(Node::nodes);
            for sig in methods.filter(|n| decl.kind == K::ClassDecl && n.kind == K::Signature) {
                let method = sig.child_tokens().find(|t| t.kind == K::Ident);
                defs.extend(method.map(|t| def(Namespace::Value, t, Some(parent), Decl::Method)));
            }
        }
        _ => {}
    }
}

/// Names a pattern binds, in source order.
pub(super) fn binders(pattern: &Node, out: &mut Vec<Token>) {
    match pattern.kind {
        K::VarPat => out.extend(pattern.child_tokens().find(|t| t.kind == K::Ident)),
        K::RecordPatField if !pattern.has_token(K::Eq) => {
            out.extend(pattern.child_tokens().find(|t| t.kind == K::Ident));
        }
        // a type annotation inside a pattern binds nothing
        _ => {
            for child in pattern.nodes().filter(|n| is_pattern(n.kind)) {
                binders(child, out);
            }
        }
    }
}

pub(super) fn is_pattern(kind: K) -> bool {
    (K::WildcardPat..=K::OrPat).contains(&kind) || kind == K::RecordPatField
}

/// A second definition of a name in the same namespace.
pub(super) fn duplicates(defs: &[Def]) -> Vec<Diagnostic> {
    let mut first: HashMap<(Namespace, &str, bool), &Def> = HashMap::new();
    let mut out = Vec::new();
    for def in defs {
        let key = (def.namespace, def.name.as_str(), def.signature);
        match first.get(&key) {
            Some(original) => out.push(
                Diagnostic::error(
                    codes::R008,
                    format!("`{}` is defined more than once", def.name),
                    def.span,
                )
                .with_related(original.span, "first defined here"),
            ),
            None => {
                first.insert(key, def);
            }
        }
    }
    out
}

/// Drops the signatures of names that have a definition, which describes
/// the entity better (`foreign`, not `let`).
pub(super) fn defined(defs: Vec<Def>) -> Vec<Def> {
    let has_definition = |s: &Def| {
        defs.iter()
            .any(|d| !d.signature && d.namespace == s.namespace && d.name == s.name)
    };
    let keep: Vec<bool> = defs
        .iter()
        .map(|d| !d.signature || !has_definition(d))
        .collect();
    defs.into_iter()
        .zip(keep)
        .filter_map(|(d, k)| k.then_some(d))
        .collect()
}

/// Transparent aliases must be acyclic: `type alias A = List A` needs a nominal type.
pub(super) fn alias_cycles(tree: &Node, text: &str, file: FileId) -> Vec<Diagnostic> {
    let mut aliases: Vec<(String, Span, Vec<String>)> = Vec::new();
    for decl in tree.nodes().filter(|n| n.kind == K::TypeDecl) {
        let is_alias = decl.has_token(K::Eq)
            && !decl.has_token(K::OpaqueKw)
            && !decl.has_token(K::RefinedKw)
            && !decl.nodes().any(|n| n.kind == K::Variant);
        let Some(name) = decl.child_tokens().find(|t| t.kind == K::Con) else {
            continue;
        };
        if is_alias {
            let mut refs = Vec::new();
            for rhs in decl.nodes().filter(|n| n.kind != K::TypeParam) {
                type_names(rhs, text, &mut refs);
            }
            let span = Span::new(file, name.start, name.end);
            aliases.push((name.text(text).to_owned(), span, refs));
        }
    }
    let index: HashMap<&str, usize> = aliases
        .iter()
        .enumerate()
        .map(|(i, a)| (a.0.as_str(), i))
        .collect();
    let edges: Vec<Vec<usize>> = aliases
        .iter()
        .map(|(_, _, refs)| {
            refs.iter()
                .filter_map(|r| index.get(r.as_str()).copied())
                .collect()
        })
        .collect();
    let mut reported = vec![false; aliases.len()];
    let mut out = Vec::new();
    for start in 0..aliases.len() {
        if reported[start] {
            continue;
        }
        let Some(cycle) = shortest_cycle(start, &edges) else {
            continue;
        };
        for &i in &cycle {
            reported[i] = true;
        }
        let chain: Vec<&str> = cycle
            .iter()
            .chain([&start])
            .map(|&i| aliases[i].0.as_str())
            .collect();
        let (name, span, _) = &aliases[start];
        let mut diagnostic = Diagnostic::error(
            codes::R007,
            format!("type alias `{name}` is cyclic: {}", chain.join(" -> ")),
            *span,
        );
        for &i in cycle.iter().skip(1) {
            diagnostic = diagnostic.with_related(
                aliases[i].1,
                format!("`{}` is part of the cycle", aliases[i].0),
            );
        }
        out.push(diagnostic.with_fix(
            format!("make `{name}` a nominal type, e.g. `type {name} = {name} of …`"),
            Vec::new(),
        ));
    }
    out
}

/// Unqualified capitalized names a type refers to.
fn type_names(node: &Node, text: &str, out: &mut Vec<String>) {
    if node.kind == K::NameType && !node.has_token(K::Dot) {
        out.extend(
            node.child_tokens()
                .filter(|t| t.kind == K::Con)
                .map(|t| t.text(text).to_owned()),
        );
    }
    for child in node.nodes() {
        type_names(child, text, out);
    }
}

/// Nodes of the shortest path `start -> … -> start`, beginning with `start`.
pub(crate) fn shortest_cycle(start: usize, edges: &[Vec<usize>]) -> Option<Vec<usize>> {
    let mut previous: Vec<Option<usize>> = vec![None; edges.len()];
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for &next in &edges[node] {
            if next == start {
                let mut path = vec![node];
                while let Some(p) = previous[*path.last().expect("non-empty")] {
                    path.push(p);
                }
                path.reverse();
                return Some(path);
            }
            if previous[next].is_none() && next != start {
                previous[next] = Some(node);
                queue.push_back(next);
            }
        }
    }
    None
}
