//! Hover for `foreign` declaration metadata words (both core contract words
//! and adapter words declared by the active package backend).

use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::core_foreign_meta;

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
        let word_tok = meta_item.child_tokens().find(|t| t.kind == K::Ident)?;
        if !(word_tok.start <= offset && offset <= word_tok.end) {
            return None;
        }
        let word = word_tok.text(text);
        let range = (word_tok.start, word_tok.end);
        if let Some(core) = core_foreign_meta(word) {
            return Some((
                format!("```flick\n{}\n```\n\n{}", core.synopsis, core.doc),
                range,
            ));
        }
        let adapter = self.analysis.foreign_backend.adapter(word)?;
        let markdown = format!(
            "```flick\n{}\n```\n\n{}\n\nBackend adapter (`{}`).",
            adapter.synopsis(),
            adapter.doc,
            self.analysis.foreign_backend.backend_id
        );
        Some((markdown, range))
    }
}
