//! Plugins: blocks `head { … }` whose code a plugin gives back. The code
//! comes as an arena of core syntax ([`flick_plugin::Arena`]); it is
//! rendered as text after the module's own, parsed and spliced in place of
//! the block, the user's code inside keeping its own place in the file.

pub mod derive;
pub mod expansion;
pub mod first_party;
pub mod render;
pub mod source;
pub mod splice;
pub mod testing;

use std::sync::Arc;

use flick_plugin::{
    Arena, Block, Completion, DeriveTarget, Diagnostic, Highlight, Hover, Piece, Plugin,
};

use source::BlockSource;

use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// A plugin as the compiler runs it: linked into the compiler, or loaded.
pub trait Expander: Send + Sync {
    /// What tells this plugin and its version apart, for caches.
    fn id(&self) -> &str;
    /// What the plugin learns from all its blocks of a group.
    fn collect(&self, blocks: &[Block]) -> Result<Vec<u8>, String>;
    /// The code `block` stands for; `source` parses the holes it asks for.
    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure>;
    /// What an editor may insert at `offset` of `block`; none when the
    /// plugin fails.
    fn complete(&self, block: &Block, state: &[u8], offset: u32) -> Vec<Completion> {
        let _ = (block, state, offset);
        Vec::new()
    }
    /// What an editor shows at `offset` of `block`.
    fn hover(&self, block: &Block, state: &[u8], offset: u32) -> Option<Hover> {
        let _ = (block, state, offset);
        None
    }
    /// How an editor colors the words of `block` that are the plugin's.
    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        let _ = (block, state);
        Vec::new()
    }
    /// The layout of `block`, whose holes `source` parses; none keeps it as
    /// written, as when the plugin fails.
    fn format(&self, block: &Block, state: &[u8], source: &mut BlockSource) -> Option<Vec<Piece>> {
        let _ = (block, state, source);
        None
    }
    /// The instance of `class` for `target` as declarations; `source`, the
    /// target's declaration, parses the holes the plugin asks for.
    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        let _ = (args, module, state, source);
        let message = format!("`{class}` has no derive in this plugin");
        Err(Failure::Refused(vec![Diagnostic::error(
            message,
            target.decl.whole(),
        )]))
    }
}

/// A plugin block as editors ask its plugin about it: completions and
/// hovers, even while the block does not expand.
#[derive(Clone)]
pub struct EditorBlock {
    /// Where its frame starts in the module text; offsets of the block
    /// count from here.
    pub start: u32,
    pub block: Block,
    /// What its plugin collected from its group.
    pub state: Arc<[u8]>,
    pub plugin: Arc<dyn Expander>,
}

impl EditorBlock {
    /// The block of `source`, for editors.
    pub fn new(source: &BlockSource, state: &[u8], plugin: &Arc<dyn Expander>) -> Self {
        EditorBlock {
            start: source.start(),
            block: source.block(),
            state: Arc::from(state),
            plugin: Arc::clone(plugin),
        }
    }

    /// Whether `offset` of the module text is in the block, its end included.
    pub fn holds(&self, offset: u32) -> bool {
        let len = u32::try_from(self.block.text.len()).unwrap_or(u32::MAX);
        self.start <= offset && offset - self.start <= len
    }
}

/// Why a plugin gave no code for a block.
#[derive(Debug)]
pub enum Failure {
    /// What the plugin found wrong, at spans of the body; none when the
    /// compiler already reported why.
    Refused(Vec<Diagnostic>),
    /// The plugin itself failed: it trapped or ran out of fuel or memory.
    Broken(String),
}

/// A plugin linked into the compiler.
pub struct Native<P> {
    id: String,
    plugin: P,
}

impl<P: Plugin> Native<P> {
    /// `plugin`, told apart by `id`: its package, since the compiler's own
    /// version covers its code.
    pub fn new(id: impl Into<String>, plugin: P) -> Self {
        Native {
            id: id.into(),
            plugin,
        }
    }
}

impl<P: Plugin + Send + Sync> Expander for Native<P> {
    fn id(&self) -> &str {
        &self.id
    }

    fn collect(&self, blocks: &[Block]) -> Result<Vec<u8>, String> {
        Ok(self.plugin.collect(blocks))
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        self.plugin
            .expand(block, state, source)
            .map_err(Failure::Refused)
    }

    fn complete(&self, block: &Block, state: &[u8], offset: u32) -> Vec<Completion> {
        self.plugin.complete(block, state, offset)
    }

    fn hover(&self, block: &Block, state: &[u8], offset: u32) -> Option<Hover> {
        self.plugin.hover(block, state, offset)
    }

    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        self.plugin.highlight(block, state)
    }

    fn format(&self, block: &Block, state: &[u8], source: &mut BlockSource) -> Option<Vec<Piece>> {
        self.plugin.format(block, state, source)
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        (self.plugin)
            .derive(target, class, args, module, state, source)
            .map_err(Failure::Refused)
    }
}

/// The plugin blocks under `node`, expressions and declarations, not those
/// inside them.
pub fn blocks_under<'n>(node: &'n Node, out: &mut Vec<&'n Node>) {
    for child in node.nodes() {
        if matches!(child.kind, K::PluginExpr | K::PluginDecl) {
            out.push(child);
        } else {
            blocks_under(child, out);
        }
    }
}

#[cfg(test)]
mod tests;
