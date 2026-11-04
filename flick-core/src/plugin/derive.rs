//! `@derive` before a class: the plugin of a package derives its instances
//! for the types that name it in `deriving`, as a block's plugin gives the
//! block's code.

use flick_plugin::{Block, CaseTarget, DeriveTarget, Span};

use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_type;

/// A class a module declares with `@derive`, and the package its attribute
/// names, `None` for the class's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub class: String,
    pub package: Option<String>,
}

/// The classes of `tree` marked `@derive`, in order.
pub fn derived_classes(tree: &Node, text: &str) -> Vec<Derived> {
    let mut out = Vec::new();
    let mut pending: Option<Option<String>> = None;
    for child in tree.nodes() {
        if child.kind == K::Attribute {
            let words: Vec<&str> = (child.child_tokens())
                .filter(|t| t.kind == K::Ident)
                .map(|t| t.text(text))
                .collect();
            pending = match words[..] {
                ["derive"] => Some(None),
                ["derive", package] => Some(Some(package.to_owned())),
                _ => pending,
            };
            continue;
        }
        if let Some(package) = pending.take()
            && child.kind == K::ClassDecl
            && let Some(class) = child.child_tokens().find(|t| t.kind == K::Con)
        {
            out.push(Derived {
                class: class.text(text).to_owned(),
                package,
            });
        }
    }
    out
}

/// What `decl`, a `TypeDecl` read as `block`, declares, for a derive: its
/// spans count from its start. `qualifier` names the module of a type of
/// another module, which qualifies its names and makes no holes.
pub fn target(
    decl: &Node,
    text: &str,
    block: Block,
    qualifier: Option<&str>,
) -> Option<DeriveTarget> {
    let start = decl.range().0;
    let span = |n: &Node| {
        let (s, e) = n.range();
        Span {
            start: s - start,
            end: e - start,
        }
    };
    let named = |name: &str| match qualifier {
        Some(q) => format!("{q}.{name}"),
        None => name.to_owned(),
    };
    let name = decl.child_tokens().find(|t| t.kind == K::Con)?.text(text);
    let params = (decl.nodes())
        .filter(|n| n.kind == K::TypeParam)
        .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Ident))
        .map(|t| t.text(text).to_owned())
        .collect();
    let cases = (decl.nodes())
        .filter(|n| n.kind == K::Variant)
        .filter_map(|v| {
            let con = v.child_tokens().find(|t| t.kind == K::Con)?.text(text);
            let fields = v.nodes().filter(|n| is_type(n.kind)).map(span).collect();
            Some(CaseTarget {
                name: named(con),
                fields,
            })
        })
        .collect();
    let body = decl
        .nodes()
        .find(|n| is_type(n.kind) && n.kind != K::TypeParam);
    let alias = decl
        .child_tokens()
        .any(|t| t.kind == K::Ident && t.text(text) == "alias");
    let refined = decl.has_token(K::RefinedKw);
    Some(DeriveTarget {
        decl: block,
        name: named(name),
        params,
        cases,
        alias: body.filter(|_| alias).map(span),
        refined: body.filter(|_| refined).map(span),
        local: qualifier.is_none(),
    })
}
