//! Editor queries a plugin answers for its blocks: what to insert at an
//! offset and what a span means. The user's code inside a block is ordinary
//! Flick, which the other queries answer.

use super::complete::item;
use super::{Completion, CompletionKind, Ide, tree};
use crate::plugin::{EditorBlock, head_token};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::show_signature;

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
        let b = self.editor_block(file, offset)?;
        let mut source = b.source();
        source.see_scope(self.in_scope(self.module(file)?));
        let offered = (b.plugin)
            .complete(&b.block, &b.state, offset - b.start, dotted, &mut source)
            .unwrap_or_else(|why| failed(b.plugin.id(), "complete", &why));
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
        let m = self.module(file)?;
        let facts =
            (m.expansion.as_ref()).map_or_else(Vec::new, |e| e.facts(b.start, &m.typing.wildcards));
        let hover = (b.plugin)
            .hover(&b.block, &b.state, offset - b.start, &facts)
            .unwrap_or_else(|why| failed(b.plugin.id(), "hover", &why))?;
        let len = u32::try_from(b.block.text.len()).ok()?;
        // a plugin's span outside its block is none
        (hover.span.start <= hover.span.end && hover.span.end <= len).then(|| {
            let range = (b.start + hover.span.start, b.start + hover.span.end);
            (hover.markdown, range)
        })
    }
}

impl Ide<'_> {
    /// On the head of a block: the type of the code it stands for, the
    /// function it gives, as the instance of a component, else its value;
    /// and what its plugin says of its head.
    pub(super) fn head_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let block = *path.iter().rev().find(|n| {
            matches!(n.kind, K::PluginExpr | K::PluginDecl)
                && head_token(n).is_some_and(|h| h.start <= offset && offset <= h.end)
        })?;
        let head = head_token(block)?;
        let expansion = m.expansion.as_ref()?;
        let code = expansion.standing_for(block.range());
        let function = |&&(start, end): &&(u32, u32)| {
            expansion
                .text
                .get(start as usize..end as usize)
                .is_some_and(|c| c.starts_with("(fun"))
        };
        // a `fun` has its type at its keyword, after the `(` of generated code
        let keyword = |&(start, _): &(u32, u32)| m.typing.types.get(&(start + 1, start + 4));
        let typed = |&(start, end): &(u32, u32)| m.typing.types.get(&(start, end));
        let ty = (code.iter().filter(function).find_map(keyword))
            .or_else(|| code.iter().find_map(typed))?;
        let mut markdown = format!(
            "```flick\n{} : {}\n```",
            head.text(text),
            show_signature(ty)
        );
        // the plugin answers for its head at an offset past the block
        let first =
            (block.tokens().into_iter()).find(|t| !t.kind.is_trivia() && t.start > head.start);
        if let Some(b) = first.and_then(|t| self.editor_block(file, t.start)) {
            let past = u32::try_from(b.block.text.len()).ok()?.saturating_add(1);
            let facts = Vec::new();
            let said = (b.plugin)
                .hover(&b.block, &b.state, past, &facts)
                .unwrap_or_else(|why| failed(b.plugin.id(), "hover", &why));
            if let Some(said) = said {
                markdown = format!("{markdown}\n\n{}", said.markdown);
            }
        }
        Some((markdown, (head.start, head.end)))
    }
}

/// What an editor gets from a plugin that failed to answer `what`: nothing,
/// and a line in the server's log.
pub(super) fn failed<T: Default>(plugin: &str, what: &str, why: &str) -> T {
    tracing::warn!(plugin, what, why, "a plugin failed to answer the editor");
    T::default()
}
