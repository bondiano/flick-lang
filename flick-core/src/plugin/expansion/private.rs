//! Names of a block's code that are its plugin's alone: a constructor with
//! `$` in its name, which no source can write, is named only by the code of
//! the plugin whose code declares it. A type over another keeps its values
//! to those its own functions made.
//!
//! The invariant of a type is its block's alone as well: `@invariant$of`,
//! the type it stands before and the companions it names are the code of
//! one block, so no plugin can give a check to another's type.

use super::Expansion;
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::Namespace;
use crate::resolve;
use crate::source::Span;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::heads::INVARIANT_OF;
use crate::syntax::kind::SyntaxKind as K;

impl Expansion {
    /// Each block whose code names such a constructor of another plugin's
    /// code.
    pub(super) fn foreign_constructors(&self) -> Vec<Diagnostic> {
        let plugin_at = |at: u32| (self.blocks.iter()).find(|b| b.code.0 <= at && at < b.code.1);
        let declared = resolve::declarations(&self.tree, &self.text, self.file);
        let owners: Vec<(&str, &str)> = (declared.iter())
            .filter(|d| d.namespace == Namespace::Constructor && d.name.contains('$'))
            .filter_map(|d| Some((d.name.as_str(), plugin_at(d.span.start)?.plugin.as_str())))
            .collect();
        if owners.is_empty() {
            return Vec::new();
        }
        let mut named = Vec::new();
        constructors(&self.tree, self.len, &mut named);
        (named.iter())
            .filter_map(|token| {
                let name = token.text(&self.text);
                let (_, owner) = owners.iter().find(|(declared, _)| *declared == name)?;
                let block = plugin_at(token.start)?;
                if block.plugin == *owner {
                    return None;
                }
                let message = format!(
                    "plugin `{}` gave code that cannot be placed: it names `{name}`, which only the code of plugin `{owner}` may name",
                    block.plugin
                );
                let (start, end) = block.range;
                Some(Diagnostic::error(codes::P036, message, Span::new(self.file, start, end)))
            })
            .collect()
    }
}

impl Expansion {
    /// Each block whose `@invariant$of` stands before a type, or names a
    /// companion, that is not the code of that block.
    pub(super) fn foreign_invariants(&self) -> Vec<Diagnostic> {
        let decls: Vec<&Node> = self.tree.nodes().collect();
        let declared = resolve::declarations(&self.tree, &self.text, self.file);
        let mut out = Vec::new();
        for (i, attribute) in decls.iter().enumerate() {
            let words: Vec<&str> = (attribute.child_tokens())
                .filter(|t| t.kind == K::Ident)
                .map(|t| t.text(&self.text))
                .collect();
            let [word, check, value, _] = words[..] else {
                continue;
            };
            if attribute.kind != K::Attribute
                || !(INVARIANT_OF.ends_with(word) && word.contains('$'))
            {
                continue;
            }
            let Some(block) = (self.blocks.iter())
                .find(|b| b.code.0 <= attribute.range().0 && attribute.range().0 < b.code.1)
            else {
                continue;
            };
            let within = |at: u32| block.code.0 <= at && at < block.code.1;
            let ty = (decls.get(i + 1))
                .filter(|d| d.kind == K::TypeDecl && within(d.range().0))
                .and_then(|d| d.child_tokens().find(|t| t.kind == K::Con))
                .map(|t| t.text(&self.text));
            let own = ty.is_some_and(|ty| {
                [check, value].iter().all(|member| {
                    let name = format!("{ty}${member}");
                    (declared.iter()).any(|d| {
                        d.namespace == Namespace::Value && d.name == name && within(d.span.start)
                    })
                })
            });
            if !own {
                let message = format!(
                    "plugin `{}` gave code that cannot be placed: its invariant stands before a type, or names a check or a value, that its block does not declare",
                    block.plugin
                );
                let (start, end) = block.range;
                out.push(Diagnostic::error(
                    codes::P036,
                    message,
                    Span::new(self.file, start, end),
                ));
            }
        }
        out
    }
}

/// The constructor tokens of generated code under `node`.
fn constructors(node: &Node, generated: u32, out: &mut Vec<Token>) {
    for child in &node.children {
        match child {
            Element::Node(n) => constructors(n, generated, out),
            Element::Token(t) if t.kind == K::Con && t.start >= generated => out.push(*t),
            Element::Token(_) => {}
        }
    }
}
