//! Editor queries a plugin answers for its blocks: what to insert at an
//! offset and what a span means. The user's code inside a block is ordinary
//! Flick, which the other queries answer.

use super::complete::item;
use super::{Completion, CompletionKind, Ide};
use crate::plugin::EditorBlock;
use crate::source::FileId;

impl Ide<'_> {
    /// The innermost plugin block of `file` at `offset`.
    fn editor_block(&self, file: FileId, offset: u32) -> Option<&EditorBlock> {
        (self.module(file)?.editing.iter())
            .filter(|b| b.holds(offset))
            .max_by_key(|b| b.start)
    }

    /// What the plugin of the block at `offset` offers there; `None` when it
    /// offers nothing, as in the user's code.
    pub(super) fn plugin_complete(
        &self,
        file: FileId,
        offset: u32,
        dotted: bool,
    ) -> Option<Vec<Completion>> {
        if dotted {
            return None;
        }
        let b = self.editor_block(file, offset)?;
        let offered = b.plugin.complete(&b.block, &b.state, offset - b.start);
        (!offered.is_empty()).then(|| {
            (offered.into_iter())
                .map(|c| Completion {
                    documentation: c.documentation,
                    snippet: c.snippet,
                    ..item(&c.label, CompletionKind::Keyword, c.detail)
                })
                .collect()
        })
    }

    /// What the plugin of the block at `offset` shows there, and its range.
    pub(super) fn plugin_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let b = self.editor_block(file, offset)?;
        let hover = b.plugin.hover(&b.block, &b.state, offset - b.start)?;
        let len = u32::try_from(b.block.text.len()).ok()?;
        // a plugin's span outside its block is none
        (hover.span.start <= hover.span.end && hover.span.end <= len).then(|| {
            let range = (b.start + hover.span.start, b.start + hover.span.end);
            (hover.markdown, range)
        })
    }
}
