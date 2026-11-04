//! The plugins linked into the compiler: those of the packages that come
//! with it, which every build has, the browser's among them.

use std::sync::Arc;

use flick_plugin::{
    Arena, Block, Compiler, Completion, DeriveTarget, Diagnostic, Highlight, Hover, Piece, Plugin,
};
use flick_plugin_cli::Cli;

use super::{Expander, Native};

/// The heads of the plugins linked into the compiler and the module that
/// exposes each. One is a block even where no import exposes it, so the
/// compiler can say which import it needs: `cli "x" { … }` stays the block
/// it was before `Cli` gave it.
pub const HEADS: [(&str, &str); 1] = [("cli", "Cli")];

/// The plugin linked into the compiler for `package`.
pub fn plugin(package: &str) -> Option<Arc<dyn Expander>> {
    match package {
        "std" => Some(Arc::new(Native::new("std", Std))),
        _ => None,
    }
}

/// The plugin of std: `cli` blocks and `deriving Cli.Value`, and the
/// derives of the witnesses of the Prelude.
struct Std;

impl Plugin for Std {
    fn collect(&self, blocks: &[Block]) -> Vec<u8> {
        Cli.collect(blocks)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        Cli.expand(block, state, compiler)
    }

    fn complete(&self, block: &Block, state: &[u8], offset: u32) -> Vec<Completion> {
        Cli.complete(block, state, offset)
    }

    fn hover(&self, block: &Block, state: &[u8], offset: u32) -> Option<Hover> {
        Cli.hover(block, state, offset)
    }

    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        Cli.highlight(block, state)
    }

    fn format(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Option<Vec<Piece>> {
        Cli.format(block, state, compiler)
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        if flick_plugin_derive::CLASSES.contains(&class) {
            return flick_plugin_derive::derive(target, class, module);
        }
        Cli.derive(target, class, args, module, state, compiler)
    }
}
