//! Private top-level values that no exported definition reaches.

use std::collections::BTreeSet;

use super::defs::Def;
use super::uses::Reference;
use super::{Decl, Interface, Module, Namespace};
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::wire::snake;

/// Warnings for the private `let`s of `module` that the declarations it
/// exports, its types and its instances never reach, however indirectly;
/// `defs` has the module's signatures too.
pub(super) fn dead_values(
    module: &Module,
    defs: &[Def],
    interface: &Interface,
    references: &[Reference],
) -> Vec<Diagnostic> {
    // a module that exports nothing is a script or a draft
    if module
        .syntax
        .header
        .as_ref()
        .is_none_or(|h| h.exposing.is_none())
    {
        return Vec::new();
    }
    let decls: Vec<&Node> = module.tree.nodes().collect();
    let exported = |name: &str| interface.find(Namespace::Value, name).is_some();
    // the one private value each top-level declaration defines
    let private: Vec<Option<&Def>> = decls
        .iter()
        .map(|decl| {
            let (start, end) = decl.range();
            // what a plugin declares stands for the user's block, which is live
            if start >= module.generated {
                return None;
            }
            let mut values = defs.iter().filter(|d| {
                d.namespace == Namespace::Value && start <= d.span.start && d.span.end <= end
            });
            match (values.next(), values.next()) {
                (Some(d), None) if d.decl == Decl::Let && !d.signature && !exported(&d.name) => {
                    Some(d)
                }
                _ => None,
            }
        })
        .collect();
    let codecs = api_codecs(&decls, module.text);
    let mut reached: BTreeSet<&str> = BTreeSet::new();
    let mut live: Vec<bool> = private.iter().map(Option::is_none).collect();
    let mut queue: Vec<usize> = (0..decls.len()).filter(|&i| live[i]).collect();
    while let Some(i) = queue.pop() {
        let named = references
            .iter()
            .filter(|r| r.top == i && r.qualifier.is_none())
            .map(|r| r.name.as_str());
        let used = codecs
            .iter()
            .filter(|(top, _)| *top == i)
            .map(|(_, n)| n.as_str());
        for name in named.chain(used) {
            if reached.insert(name) {
                for (j, def) in private.iter().enumerate() {
                    if !live[j] && def.is_some_and(|d| d.name == name) {
                        live[j] = true;
                        queue.push(j);
                    }
                }
            }
        }
    }
    private
        .iter()
        .enumerate()
        .filter(|&(i, _)| !live[i])
        .filter_map(|(i, def)| Some((decls[i], (*def)?)))
        .filter(|(_, def)| !def.name.starts_with('_'))
        .map(|(decl, def)| {
            Diagnostic::new(
                codes::R014,
                Severity::Warning,
                format!("`{}` is never used", def.name),
            )
            .with_span(def.span)
            .with_fix(|| {
                let signature = defs
                    .iter()
                    .find(|d| d.signature && d.namespace == Namespace::Value && d.name == def.name);
                (
                    format!("remove `{}`", def.name),
                    vec![removal(module, decl, signature)],
                )
            })
            .with_fix(|| {
                (
                    format!("expose `{}` from the module", def.name),
                    expose_value(module, &def.name).into_iter().collect(),
                )
            })
        })
        .collect()
}

fn expose_value(module: &Module, name: &str) -> Option<Edit> {
    let header = module.tree.nodes().find(|n| n.kind == K::ModuleHeader)?;
    let exposing = header.nodes().find(|n| n.kind == K::Exposing)?;
    let items: Vec<_> = exposing
        .nodes()
        .filter(|n| n.kind == K::ExposeItem)
        .collect();
    let (at, replacement) = match items.last() {
        Some(last) => (last.range().1, format!(", {name}")),
        None => {
            let close = exposing.child_tokens().find(|t| t.kind == K::RParen)?;
            (close.start, name.to_owned())
        }
    };
    Some(Edit {
        span: Span::new(module.file, at, at),
        replacement,
    })
}

/// The text of a declaration, its signature and the line breaks after it.
fn removal(module: &Module, decl: &Node, signature: Option<&Def>) -> Edit {
    let (start, end) = decl.range();
    let start = signature.map_or(start, |s| {
        let line = module.text[..s.span.start as usize]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        start.min(u32::try_from(line).expect("source offsets fit in u32"))
    });
    let rest = &module.text[end as usize..];
    let breaks = rest.len() - rest.trim_start_matches('\n').len();
    let end = end + u32::try_from(breaks.min(2)).expect("small");
    Edit {
        span: Span::new(module.file, start, end),
        replacement: String::new(),
    }
}

/// The codecs a record that derives `HttpApi` uses by name: the value named
/// after each type its fields write, by declaration.
fn api_codecs(decls: &[&Node], text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, decl) in decls.iter().enumerate() {
        let derives = decl
            .nodes()
            .filter(|n| n.kind == K::DerivingClause)
            .flat_map(Node::child_tokens)
            .any(|t| t.kind == K::Con && t.text(text) == "HttpApi");
        if !derives {
            continue;
        }
        let mut stack: Vec<&Node> = decl.nodes().collect();
        while let Some(node) = stack.pop() {
            stack.extend(node.nodes());
            if node.kind == K::NameType
                && let Some(last) = node.child_tokens().filter(|t| t.kind == K::Con).last()
            {
                out.push((i, snake(last.text(text))));
            }
        }
    }
    out
}
