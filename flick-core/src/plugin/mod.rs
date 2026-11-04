//! Plugins: blocks `head { … }` whose code a plugin gives back. The code
//! comes as an arena of core syntax ([`flick_plugin::Arena`]); it is
//! rendered as text after the module's own, parsed and spliced in place of
//! the block, the user's code inside keeping its own place in the file.

pub mod expansion;
pub mod render;
pub mod source;
pub mod splice;

use flick_plugin::{Arena, Block, Diagnostic, Plugin};

use source::BlockSource;

/// A plugin as the compiler runs it: linked into the compiler, or loaded.
pub trait Expander {
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

impl<P: Plugin> Expander for Native<P> {
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
}

#[cfg(test)]
mod tests;
