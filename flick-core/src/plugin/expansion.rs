//! A module with its blocks expanded: the text checking and lowering read
//! (the module's text, then the code of each block on its own line), the
//! tree with each block replaced by its code, a tree for editors with the
//! user's code inside blocks parsed in place, and the way back from a place
//! in generated code to the place in the module it stands for.

use std::mem;

use crate::diagnostics::{Diagnostic, Related, Severity, codes};
use crate::ir;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

use super::render::Rendered;
use super::source::BlockSource;
use super::splice::splice;

/// A block and the code its plugin gave for it.
pub struct Expanded {
    pub node: Node,
    pub rendered: Rendered,
    pub source: BlockSource,
    pub plugin: String,
}

pub struct Expansion {
    pub text: String,
    /// The length of the module's own text: offsets past it are generated.
    pub len: u32,
    pub tree: Node,
    /// The module's tree with the user's code of each block parsed in place,
    /// all at the module's own offsets.
    pub source_tree: Node,
    file: FileId,
    /// Each range of generated code and the range of the module it stands for.
    origins: Vec<((u32, u32), (u32, u32))>,
    /// The range of each expanded block, the code of it and its plugin.
    blocks: Vec<Block>,
}

struct Block {
    range: (u32, u32),
    code: (u32, u32),
    plugin: String,
}

impl Expansion {
    /// The module `text` of `file`, parsed into `tree`, with `blocks`
    /// expanded; a block whose code cannot be placed is reported and left
    /// out. `None` when no block is left.
    pub fn build(
        text: &str,
        file: FileId,
        tree: &Node,
        blocks: Vec<Expanded>,
    ) -> (Option<Expansion>, Vec<Diagnostic>) {
        let len = u32::try_from(text.len()).expect("source fits in u32");
        let mut out = text.to_owned();
        let mut diagnostics = Vec::new();
        let mut done: Vec<Block> = Vec::new();
        let mut origins = Vec::new();
        let mut spliced = Vec::new();
        let mut holes = Vec::new();
        for block in blocks {
            out.push('\n');
            let at = u32::try_from(out.len()).expect("source fits in u32");
            out.push_str(&block.rendered.text);
            let range = block.node.range();
            match splice(&out, file, at, &block.rendered, &block.source, &block.node) {
                Ok(node) => {
                    let start = block.source.start();
                    origins.extend(
                        block.rendered.origins.iter().map(|&((s, e), o)| {
                            ((at + s, at + e), (start + o.start, start + o.end))
                        }),
                    );
                    holes.extend(
                        block
                            .rendered
                            .holes
                            .iter()
                            .filter_map(|&(_, hole)| Some((range, block.source.hole_tree(hole)?))),
                    );
                    let end = u32::try_from(out.len()).expect("source fits in u32");
                    done.push(Block {
                        range,
                        code: (at, end),
                        plugin: block.plugin,
                    });
                    spliced.push((range, node));
                }
                Err(why) => diagnostics.push(Diagnostic::error(
                    codes::P036,
                    format!(
                        "plugin `{}` gave code that cannot be placed: {why}",
                        block.plugin
                    ),
                    Span::new(file, range.0, range.1),
                )),
            }
        }
        if done.is_empty() {
            return (None, diagnostics);
        }
        let mut expanded = tree.clone();
        replace_blocks(&mut expanded, &mut spliced);
        let mut source_tree = tree.clone();
        parse_holes(&mut source_tree, &holes);
        let expansion = Expansion {
            text: out,
            len,
            tree: expanded,
            source_tree,
            file,
            origins,
            blocks: done,
        };
        (Some(expansion), diagnostics)
    }

    /// The span of the module a span of the expansion stands for.
    pub fn place(&self, span: Span) -> Span {
        if span.file != self.file || span.end <= self.len {
            return span;
        }
        let start = if span.start < self.len {
            span.start
        } else {
            self.origin(span.start).0
        };
        let last = span.end.saturating_sub(1).max(span.start);
        let end = self.origin(last).1.max(start);
        Span::new(span.file, start, end)
    }

    /// Diagnostics and IR of the expanded module at the places of the module
    /// they stand for. Warnings about generated code and fixes that edit it
    /// are dropped: they are about the plugin's code, not the user's.
    pub fn restore(&self, diagnostics: &mut Vec<Diagnostic>, ir: Option<&mut ir::Module>) {
        diagnostics.retain_mut(|d| {
            let Some(span) = d.span.filter(|s| s.file == self.file) else {
                return true;
            };
            let block = self.block_of(span.start);
            let at_block = self
                .blocks
                .iter()
                .any(|b| (span.start, span.end) == b.range);
            if d.severity != Severity::Error && (block.is_some() || at_block) {
                return false;
            }
            for related in &mut d.related {
                related.span = self.place(related.span);
            }
            #[cfg(feature = "diagnostic-fixes")]
            d.fixes.retain(|fix| {
                fix.edits
                    .iter()
                    .all(|e| e.span.file != self.file || e.span.end <= self.len)
            });
            if let Some(block) = block {
                let placed = self.place(span);
                // generated code for nothing written, as of an empty body
                let whole = Span::new(self.file, block.range.0, block.range.1);
                d.span = Some(if placed.start == placed.end {
                    whole
                } else {
                    placed
                });
                d.related.push(Related {
                    span: Span::new(self.file, block.range.0, block.range.1),
                    message: format!("in the code plugin `{}` gave for this block", block.plugin),
                });
            }
            true
        });
        if let Some(ir) = ir {
            ir.each_span_mut(&mut |s| *s = self.place(*s));
        }
    }

    /// The block whose generated code is at `offset`.
    fn block_of(&self, offset: u32) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|b| b.code.0 <= offset && offset < b.code.1)
    }

    /// The range of the module the innermost generated code at `offset`
    /// stands for.
    fn origin(&self, offset: u32) -> (u32, u32) {
        self.origins
            .iter()
            .filter(|((s, e), _)| *s <= offset && offset < *e)
            .min_by_key(|((s, e), _)| e - s)
            .map(|&(_, origin)| origin)
            .or_else(|| self.block_of(offset).map(|b| b.range))
            .unwrap_or((self.len, self.len))
    }
}

/// Replaces each block under `node` by its code from `spliced`, by range.
fn replace_blocks(node: &mut Node, spliced: &mut Vec<((u32, u32), Node)>) {
    for child in &mut node.children {
        let Element::Node(n) = child else {
            continue;
        };
        if n.kind == K::PluginExpr {
            let range = n.range();
            if let Some(i) = spliced.iter().position(|(r, _)| *r == range) {
                *n = spliced.swap_remove(i).1;
            }
            continue;
        }
        replace_blocks(n, spliced);
    }
}

/// Replaces the tokens of the user's code inside each block under `node` by
/// that code parsed, from `holes` (a block's range and a tree of its code).
fn parse_holes(node: &mut Node, holes: &[((u32, u32), Node)]) {
    for child in &mut node.children {
        let Element::Node(n) = child else {
            continue;
        };
        if n.kind != K::PluginExpr {
            parse_holes(n, holes);
            continue;
        }
        let range = n.range();
        let mut trees: Vec<&Node> = holes
            .iter()
            .filter(|(r, _)| *r == range)
            .map(|(_, t)| t)
            .collect();
        trees.sort_by_key(|t| t.range());
        let Some(Element::Node(body)) = n
            .children
            .iter_mut()
            .find(|c| matches!(c, Element::Node(b) if b.kind == K::Block))
        else {
            continue;
        };
        let mut children = Vec::new();
        let mut next = trees.into_iter().peekable();
        for token in mem::take(&mut body.children) {
            let Element::Token(t) = token else {
                children.push(token);
                continue;
            };
            match next.peek() {
                Some(tree) if tree.range().0 <= t.start && t.end <= tree.range().1 => {
                    if tree.range().1 == t.end {
                        children.push(Element::Node((*tree).clone()));
                        next.next();
                    }
                }
                _ => children.push(Element::Token(t)),
            }
        }
        body.children = children;
    }
}
