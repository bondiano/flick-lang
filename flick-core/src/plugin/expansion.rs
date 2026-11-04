//! A module with its blocks expanded: the text checking and lowering read
//! (the module's text, then the code of each block on its own line), the
//! tree with each block replaced by its code, a tree for editors with the
//! user's code inside blocks parsed in place, and the way back from a place
//! in generated code to the place in the module it stands for.

use std::iter::Peekable;
use std::mem;

use flick_plugin::{Hole, Place};

use crate::diagnostics::{Diagnostic, Related, Severity, codes};
use crate::iface::{Interface, Namespace};
use crate::ir;
use crate::resolve;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

use super::render::Rendered;
use super::source::BlockSource;
use super::splice::{splice, splice_decls};
use super::testing::shown;

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
    /// Where its frame is in the module text, where it stands, and the
    /// holes of its code.
    frame: (u32, u32),
    place: Place,
    holes: Vec<(u32, Hole)>,
}

/// A block and the code its plugin gave, as Flick text with the user's code
/// in place.
pub struct Shown {
    pub range: (u32, u32),
    pub plugin: String,
    pub code: String,
}

impl Expansion {
    /// Each expanded block, as [`shown`] shows it.
    pub fn shown(&self) -> Vec<Shown> {
        (self.blocks.iter())
            .map(|b| {
                let rendered = Rendered {
                    text: self.text[b.code.0 as usize..b.code.1 as usize].to_owned(),
                    origins: Vec::new(),
                    holes: b.holes.clone(),
                };
                let frame = &self.text[b.frame.0 as usize..b.frame.1 as usize];
                Shown {
                    range: b.range,
                    plugin: b.plugin.clone(),
                    code: shown(&rendered, frame, b.place),
                }
            })
            .collect()
    }

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
            let placed = match block.source.place() {
                Place::Expression => {
                    splice(&out, file, at, &block.rendered, &block.source, &block.node)
                        .map(|node| vec![node])
                }
                Place::Declaration => splice_decls(&out, file, at, &block.rendered, &block.source),
            };
            match placed {
                Ok(nodes) => {
                    let start = block.source.start();
                    let frame = block.source.end() - start;
                    origins.extend(block.rendered.origins.iter().map(|&((s, e), o)| {
                        // the whole frame is the whole block, its head with it
                        let origin = if (o.start, o.end) == (0, frame) {
                            range
                        } else {
                            (start + o.start, start + o.end)
                        };
                        ((at + s, at + e), origin)
                    }));
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
                        frame: (block.source.start(), block.source.end()),
                        place: block.source.place(),
                        holes: block.rendered.holes.clone(),
                    });
                    spliced.push((range, nodes));
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
        diagnostics.extend(expansion.stray_companions());
        (Some(expansion), diagnostics)
    }

    /// The top-level names the code of the module's blocks declares, each at
    /// what it stands for in the module: its block, or the user's name there
    /// the plugin gave as its origin.
    pub fn declarations(&self) -> Vec<resolve::Declared> {
        (resolve::declarations(&self.tree, &self.text, self.file).into_iter())
            .filter(|d| d.span.start >= self.len)
            .map(|d| resolve::Declared {
                span: self.place(d.span),
                ..d
            })
            .collect()
    }

    /// The plugin whose code declares what stands at `span` of the module,
    /// which a block of it declares.
    pub fn plugin_at(&self, span: Span) -> Option<&str> {
        (self.blocks.iter())
            .find(|b| b.range.0 <= span.start && span.end <= b.range.1)
            .map(|b| b.plugin.as_str())
    }

    /// Each companion `Owner.member` the code of a block declares whose
    /// owner is no type of the module.
    fn stray_companions(&self) -> Vec<Diagnostic> {
        let declared = resolve::declarations(&self.tree, &self.text, self.file);
        let is_type = |name: &str| {
            (declared.iter()).any(|d| d.namespace == Namespace::Type && d.name == name)
        };
        (declared.iter())
            .filter(|d| d.namespace == Namespace::Value && !d.signature)
            .filter_map(|d| Some((d, d.name.split_once('$')?)))
            .filter(|(_, (owner, _))| !is_type(owner))
            .filter_map(|(d, (owner, member))| {
                let block = (self.blocks.iter())
                    .find(|b| b.code.0 <= d.span.start && d.span.start < b.code.1)?;
                let message = format!(
                    "plugin `{}` declares `{owner}.{member}`, but `{owner}` is no type of this module",
                    block.plugin
                );
                let (start, end) = block.range;
                Some(Diagnostic::error(codes::P036, message, Span::new(self.file, start, end)))
            })
            .collect()
    }

    /// Where the module's text declares the names of `interface`: a name the
    /// code of a block declares, at the block.
    pub fn place_interface(&self, interface: &mut Interface) {
        for export in &mut interface.exports {
            if let Some([start, end]) = export.at {
                let placed = self.place(Span::new(self.file, start, end));
                export.at = Some([placed.start, placed.end]);
            }
        }
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

/// Replaces each block under `node` by its code from `spliced`, by range:
/// an expression by its tree, a declaration of the module by its
/// declarations.
fn replace_blocks(node: &mut Node, spliced: &mut Vec<((u32, u32), Vec<Node>)>) {
    let mut children = Vec::with_capacity(node.children.len());
    for child in mem::take(&mut node.children) {
        let Element::Node(mut n) = child else {
            children.push(child);
            continue;
        };
        if matches!(n.kind, K::PluginExpr | K::PluginDecl) {
            let range = n.range();
            if let Some(i) = spliced.iter().position(|(r, _)| *r == range) {
                // the user's code in its code has blocks of its own
                for mut code in spliced.swap_remove(i).1 {
                    replace_blocks(&mut code, spliced);
                    children.push(Element::Node(code));
                }
                continue;
            }
        } else {
            replace_blocks(&mut n, spliced);
        }
        children.push(Element::Node(n));
    }
    node.children = children;
}

/// Replaces the tokens of the user's code inside each block under `node` by
/// that code parsed, from `holes` (a block's range and a tree of its code).
fn parse_holes(node: &mut Node, holes: &[((u32, u32), Node)]) {
    for child in &mut node.children {
        let Element::Node(n) = child else {
            continue;
        };
        if !matches!(n.kind, K::PluginExpr | K::PluginDecl) {
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
        let mut next = trees.into_iter().peekable();
        n.children = in_place(mem::take(&mut n.children), &mut next);
        // the blocks inside that code
        parse_holes(n, holes);
    }
}

/// `elements` of a block with the tokens of each tree `next` gives replaced
/// by the tree: its string arguments, and inside `( … )` and the body.
fn in_place<'t>(
    elements: Vec<Element>,
    next: &mut Peekable<impl Iterator<Item = &'t Node>>,
) -> Vec<Element> {
    let mut children = Vec::new();
    for element in elements {
        let t = match element {
            Element::Token(t) => t,
            Element::Node(mut raw)
                if matches!(raw.kind, K::PluginArgs | K::Block | K::PluginRest) =>
            {
                raw.children = in_place(mem::take(&mut raw.children), next);
                children.push(Element::Node(raw));
                continue;
            }
            other @ Element::Node(_) => {
                children.push(other);
                continue;
            }
        };
        // a tree that took no token of the block is none of its own
        while next.peek().is_some_and(|tree| tree.range().1 <= t.start) {
            next.next();
        }
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
    children
}
