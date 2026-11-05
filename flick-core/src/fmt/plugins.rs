//! Plugin blocks. A block whose plugin lays it out (`format`) is printed
//! as its plugin says, the user's code in its holes laid out as any other;
//! another stays as written, the lines after its first moving with its head.

use std::collections::HashMap;
use std::slice;
use std::sync::Arc;

use flick_plugin::Piece;

use super::printer::{Doc, INDENT, Printer};
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, blocks_under, first_party};
use crate::resolve;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_std_module;

/// The plugin a file's head stands for, when the caller knows it; the
/// plugin of `std` is known without.
pub type PluginOf<'p> = &'p dyn Fn(&str) -> Option<Arc<dyn Expander>>;

/// The layout of a block its plugin gives, and the user's code of each of
/// its holes, in order.
pub(super) struct Layout {
    pieces: Vec<Piece>,
    holes: Vec<Node>,
    /// The layout starts in the body, after a `{` the compiler writes.
    opens: bool,
    /// The block ends with a `}` the compiler writes.
    closes: bool,
}

/// The layout of each block of a file, by its range.
pub(super) type Layouts = HashMap<(u32, u32), Layout>;

/// The heads of plugin blocks the imports of `tree` expose.
pub(super) fn heads(tree: &Node, text: &str, file: FileId) -> Arc<[String]> {
    resolve::module_syntax(tree, text, file).heads()
}

/// The layouts the plugins of the blocks of `tree` give, those of blocks
/// inside the holes of others too.
pub(super) fn layouts(tree: &Node, text: &str, file: FileId, plugin_of: PluginOf) -> Layouts {
    let syntax = resolve::module_syntax(tree, text, file);
    let modules: HashMap<&str, &str> = (syntax.imports.iter())
        .flat_map(|i| {
            i.plugins
                .iter()
                .map(|p| (p.name.as_str(), i.module.as_str()))
        })
        .chain(first_party::HEADS.iter().copied())
        .collect();
    let plugin = |head: &str| {
        plugin_of(head).or_else(|| {
            let module = modules.get(head)?;
            is_std_module(module)
                .then(|| first_party::plugin("std"))
                .flatten()
        })
    };
    let shared: Arc<str> = Arc::from(text);
    let heads = syntax.heads();
    let mut out = Layouts::new();
    let mut blocks = Vec::new();
    blocks_under(tree, &mut blocks);
    let mut pending: Vec<Node> = blocks.into_iter().cloned().collect();
    while let Some(node) = pending.pop() {
        let head = node.child_tokens().find(|t| t.kind == K::Ident);
        let Some(plugin) = head.and_then(|h| plugin(h.text(text))) else {
            continue;
        };
        let Some(source) = BlockSource::new(Arc::clone(&shared), file, &node, Arc::clone(&heads))
        else {
            continue;
        };
        if let Some(layout) = layout(plugin.as_ref(), source, &node) {
            for hole in &layout.holes {
                if matches!(hole.kind, K::PluginExpr | K::PluginDecl) {
                    pending.push(hole.clone());
                } else {
                    let mut inner = Vec::new();
                    blocks_under(hole, &mut inner);
                    pending.extend(inner.into_iter().cloned());
                }
            }
            out.insert(node.range(), layout);
        }
    }
    out
}

/// The layout `plugin` gives the block `node`, read as `source`; a block
/// with comments of its own but `///` lines keeps its text, which has them.
fn layout(plugin: &dyn Expander, mut source: BlockSource, node: &Node) -> Option<Layout> {
    let commented = (node.tokens().iter()).any(|t| t.kind.is_comment() && t.kind != K::DocComment);
    if !source.closed() || commented {
        return None;
    }
    let block = source.block();
    let state = (plugin.collect(slice::from_ref(&block), &mut BlockSource::default())).ok()?;
    let pieces = plugin.format(&block, &state, &mut source)?;
    let holes = (pieces.iter())
        .filter_map(|p| match p {
            Piece::Hole(h) => Some(source.hole_tree(*h)),
            _ => None,
        })
        .collect::<Option<Vec<Node>>>()?;
    let open = node.child_tokens().find(|t| t.kind == K::LBrace);
    Some(Layout {
        pieces,
        holes,
        opens: open.is_some_and(|o| o.end == source.start()),
        closes: node.has_token(K::RBrace),
    })
}

impl<'a> Printer<'a> {
    /// A plugin block: as its plugin lays it out, else as written.
    pub fn plugin_block(&mut self, n: &'a Node) -> Doc<'a> {
        match self.layouts.get(&n.range()) {
            Some(layout) => self.laid_out(n, layout),
            None => self.shifted(n),
        }
    }

    /// `head`, then the pieces of `layout`, its holes laid out here.
    fn laid_out(&mut self, n: &'a Node, layout: &'a Layout) -> Doc<'a> {
        let a = self.a;
        // what comments the block has are the plugin's words or are dropped,
        // which the check of the declaration then refuses
        self.comments.skip(n.range().1);
        let head = n
            .child_tokens()
            .find(|t| t.kind == K::Ident)
            .map_or("", |t| t.text(self.text));
        let mut levels: Vec<Vec<Doc<'a>>> = vec![vec![self.str(head), a.text(" ")]];
        if layout.opens {
            levels[0].push(a.text("{"));
        }
        let mut holes = layout.holes.iter();
        for piece in &layout.pieces {
            match piece {
                Piece::Text(t) => {
                    let doc = self.str(t);
                    levels.last_mut().expect("a level").push(doc);
                }
                Piece::Line => levels.last_mut().expect("a level").push(a.line()),
                Piece::Indent => levels.push(Vec::new()),
                Piece::Dedent if levels.len() > 1 => {
                    let inner = levels.pop().expect("a level");
                    let doc = a.concat(inner).nest(a, INDENT);
                    levels.last_mut().expect("a level").push(doc);
                }
                Piece::Dedent => {}
                Piece::Hole(_) => {
                    let Some(tree) = holes.next() else {
                        continue;
                    };
                    let doc = self.node(tree).group(a);
                    levels.last_mut().expect("a level").push(doc);
                }
            }
        }
        while levels.len() > 1 {
            let inner = levels.pop().expect("a level");
            let doc = a.concat(inner).nest(a, INDENT);
            levels.last_mut().expect("a level").push(doc);
        }
        let mut docs = levels.pop().expect("a level");
        if layout.closes {
            docs.push(a.text("}"));
        }
        let doc = a.concat(docs);
        if layout.pieces.contains(&Piece::Line) {
            doc.force_break(a)
        } else {
            doc
        }
    }

    /// The block as written, without whitespace at line ends: each line
    /// after the first keeps its indent from the head's line, wherever that
    /// moves.
    fn shifted(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let (start, end) = n.range();
        self.comments.skip(end);
        let line = self.text[..start as usize].rfind('\n').map_or(0, |i| i + 1);
        let base = self.text[line..start as usize].len()
            - self.text[line..start as usize].trim_start().len();
        let mut lines = self.text[start as usize..end as usize].lines();
        let first = self.str(lines.next().unwrap_or_default().trim_end());
        let mut docs = vec![first];
        for l in lines {
            let indent = l.len() - l.trim_start().len();
            let cut = indent.min(base);
            docs.push(a.line());
            docs.push(self.str(l[cut..].trim_end()));
        }
        a.concat(docs)
    }
}
