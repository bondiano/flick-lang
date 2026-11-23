//! Hover documentation for foreign metadata lines: core contract words and
//! backend-declared adapter words read from the package's backend descriptor.

use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

fn core_word_doc(word: &str) -> Option<&'static str> {
    match word {
        "arity" => Some(
            "Declares the number of positional arguments the foreign host function takes. `arity 0` consumes a single `Unit` parameter in Flick and calls the host with no arguments.",
        ),
        "catch" => Some(
            "Maps a host exception or rejected promise into a typed error in the declaration's error row (`catch fn`, where `fn : JsValue -> E`).",
        ),
        "retention" => Some(
            "Declares how long the foreign call may retain capabilities from a scope: `retention call` keeps nothing past the call; `retention owner` keeps them until the owning scope closes.",
        ),
        "nullable" => Some(
            "Adapts a nullable host return value (`null` / `undefined` / `nil`) into `Option a`.",
        ),
        "optional" => Some(
            "Marks the `Option a` parameter at 0-based index `i` as optional: `None` is omitted / passed as absent to the host call.",
        ),
        "reviewed" => Some(
            "Records the audited host package version and content fingerprint that this foreign binding was reviewed against.",
        ),
        "owned" => Some(
            "Marks the parameter at 0-based index `i` as ownership-transferred to the host callee (`owned i`). Reference-counting targets consume the caller's ownership; GC targets accept and ignore it.",
        ),
        _ => None,
    }
}

impl Ide<'_> {
    pub(super) fn foreign_meta_hover(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let meta_item = path.iter().rev().find(|n| n.kind == K::MetaItem)?;
        let name_tok = meta_item.child_tokens().find(|t| t.kind == K::Ident)?;
        let word = name_tok.text(text);
        let on_name = name_tok.start <= offset && offset < name_tok.end;
        // Hovering the expression after `catch` should hover that Flick symbol.
        if word == "catch" && !on_name {
            return None;
        }
        let doc = if let Some(core) = core_word_doc(word) {
            core
        } else if let Some(spec) = m.foreign_meta.adapter_word(word) {
            spec.help.as_str()
        } else {
            return None;
        };
        let range = if on_name {
            (name_tok.start, name_tok.end)
        } else {
            meta_item
                .nodes()
                .find(|arg| {
                    let (s, e) = arg.range();
                    s <= offset && offset < e
                })
                .map(Node::range)?
        };
        let (start, end) = meta_item.range();
        let code = text[start as usize..end as usize].trim();
        Some((format!("```flick\n{code}\n```\n\n{doc}"), range))
    }
}
