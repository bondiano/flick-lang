//! The plugins linked into the compiler: those of the packages that come
//! with it, which every build has, the browser's among them.

use std::sync::Arc;

use flick_plugin::{
    Arena, Block, Compiler, Completion, DeriveTarget, Diagnostic, Highlight, Hover, Piece, Plugin,
};
use flick_plugin_cli::Cli;
use flick_plugin_derive::server_fn;

use super::{Expander, Native};

/// The heads of the plugins linked into the compiler and the module that
/// exposes each. One is a block even where no import exposes it, so the
/// compiler can say which import it needs: `cli "x" { … }` stays the block
/// it was before `Cli` gave it.
pub const HEADS: [(&str, &str); 2] = [("cli", "Cli"), ("server", "Server")];

/// The heads whose blocks need no body even where no import exposes them,
/// as the compiler read them before: `server fn f x = …` and `server f`.
pub const BARE: [&str; 1] = ["server"];

/// The plugin linked into the compiler for `package`.
pub fn plugin(package: &str) -> Option<Arc<dyn Expander>> {
    match package {
        "std" => Some(Arc::new(Native::new("std", Std))),
        _ => None,
    }
}

/// The plugin of std: `cli` blocks and `deriving Cli.Value`, `server fn`,
/// and the derives of the witnesses of the Prelude.
struct Std;

/// Whether `block` is a `cli` block, the others being `server`'s.
fn cli(block: &Block) -> bool {
    block.head != "server"
}

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
        if cli(block) {
            Cli.expand(block, state, compiler)
        } else {
            server_fn::expand(block, compiler)
        }
    }

    fn complete(&self, block: &Block, state: &[u8], offset: u32) -> Vec<Completion> {
        if cli(block) {
            Cli.complete(block, state, offset)
        } else {
            Vec::new()
        }
    }

    fn hover(&self, block: &Block, state: &[u8], offset: u32) -> Option<Hover> {
        cli(block)
            .then(|| Cli.hover(block, state, offset))
            .flatten()
    }

    fn highlight(&self, block: &Block, state: &[u8]) -> Vec<Highlight> {
        if cli(block) {
            Cli.highlight(block, state)
        } else {
            Vec::new()
        }
    }

    fn format(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Option<Vec<Piece>> {
        cli(block)
            .then(|| Cli.format(block, state, compiler))
            .flatten()
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
