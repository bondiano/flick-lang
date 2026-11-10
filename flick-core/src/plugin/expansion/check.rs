//! A block's `check`: what its plugin finds wrong with its code from the
//! types the checker found for the `_` of that code.

use crate::syntax::{
    cst::{Element, Node},
    kind::SyntaxKind as K,
    structure,
};
use crate::types::Thresholds;
use flick_plugin::{Fact, HoleKind, Span as BlockSpan};

use super::Expansion;
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::{EditorBlock, placed};
use crate::source::Span;
use crate::types::Range;

impl Expansion {
    /// The errors the plugin of each expanded block of `editing` finds from
    /// `wildcards`, the type of each `_` of the expanded text: each fact at
    /// the span of its block the plugin gave its `_`, the whole block when
    /// that is the whole frame. Source hooks also run without a `_`.
    pub fn check_blocks(
        &self,
        editing: &[EditorBlock],
        wildcards: &[(Range, Fact)],
        limits: Thresholds,
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for block in &self.blocks {
            let Some(editor) = editing.iter().find(|e| e.start == block.frame.0) else {
                continue;
            };
            let (start, end) = block.frame;
            let facts = self.facts(start, wildcards);
            // Source holes only: patterns and generated wrappers do not count.
            let mut nodes = Vec::new();
            for (_, hole) in &block.holes {
                if !matches!(hole.kind, HoleKind::Expr | HoleKind::Decl) {
                    continue;
                }
                let range = (start + hole.span.start, start + hole.span.end);
                if let Some(node) = source_node(&self.source_tree, range)
                    && !nodes.iter().any(|n: &Node| n.range() == node.range())
                {
                    nodes.push(node.clone());
                }
            }
            nodes.sort_by_key(Node::range);
            let structure = if nodes.is_empty() {
                None
            } else {
                let body = Node {
                    kind: K::Block,
                    children: nodes.into_iter().map(Element::Node).collect(),
                };
                let mut structure = structure::measure(&body, &self.text, limits);
                for n in &mut structure.nesting {
                    n.span.start -= start;
                    n.span.end -= start;
                }
                Some(structure)
            };
            let whole = Span::new(self.file, block.range.0, block.range.1);
            let found = if facts.is_empty() {
                Ok(Vec::new())
            } else {
                editor.plugin.check(&editor.block, &editor.state, &facts)
            }
            .and_then(|mut found| {
                if let Some(structure) = &structure {
                    found.extend(editor.plugin.check_structure(&editor.block, structure)?);
                }
                Ok(found)
            });
            match found {
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

    /// The facts of the block whose frame starts at `start`: each `_` of its
    /// code in `wildcards`, at the span of the block its code gave it, the
    /// whole block when that is the whole frame.
    pub fn facts(&self, start: u32, wildcards: &[(Range, Fact)]) -> Vec<Fact> {
        let Some(block) = self.blocks.iter().find(|b| b.frame.0 == start) else {
            return Vec::new();
        };
        let end = block.frame.1;
        (wildcards.iter())
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
            .collect()
    }
}

fn source_node(node: &Node, range: Range) -> Option<&Node> {
    if node.range() == range {
        return Some(node);
    }
    node.nodes()
        .filter(|child| {
            let (start, end) = child.range();
            start <= range.0 && range.1 <= end
        })
        .find_map(|child| source_node(child, range))
}
