//! A block's `check`: what its plugin finds wrong with its code from the
//! types the checker found for the `_` of that code.

use flick_plugin::{Fact, Span as BlockSpan};

use super::Expansion;
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::{EditorBlock, placed};
use crate::source::Span;
use crate::types::Range;

impl Expansion {
    /// The errors the plugin of each expanded block of `editing` finds from
    /// `wildcards`, the type of each `_` of the expanded text: each fact at
    /// the span of its block the plugin gave its `_`, the whole block when
    /// that is the whole frame. A block without a `_` is not asked.
    pub fn check_blocks(
        &self,
        editing: &[EditorBlock],
        wildcards: &[(Range, Fact)],
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for block in &self.blocks {
            let Some(editor) = editing.iter().find(|e| e.start == block.frame.0) else {
                continue;
            };
            let (start, end) = block.frame;
            let facts: Vec<Fact> = (wildcards.iter())
                .filter(|((s, e), _)| block.code.0 <= *s && *e <= block.code.1)
                .map(|&((s, e), ref fact)| {
                    let at = self.place(Span::new(self.file, s, e));
                    let span = if (at.start, at.end) == block.range {
                        BlockSpan {
                            start: 0,
                            end: end - start,
                        }
                    } else {
                        BlockSpan {
                            start: at.start.saturating_sub(start),
                            end: at.end.saturating_sub(start),
                        }
                    };
                    Fact {
                        span,
                        ..fact.clone()
                    }
                })
                .collect();
            if facts.is_empty() {
                continue;
            }
            let whole = Span::new(self.file, block.range.0, block.range.1);
            match editor.plugin.check(&editor.block, &editor.state, &facts) {
                Ok(found) => {
                    let frame = Span::new(self.file, start, end);
                    out.extend(
                        found
                            .iter()
                            .map(|d| placed(d, &block.plugin, frame, whole, false)),
                    );
                }
                Err(why) => out.push(Diagnostic::error(
                    codes::P036,
                    format!("plugin `{}` failed: {why}", block.plugin),
                    whole,
                )),
            }
        }
        out
    }
}
