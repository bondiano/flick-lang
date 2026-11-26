//! The plugin of `std` for `suite "name" { … }` and `test "name" { … }`:
//! the cases of a module as calls of `Test`, which `flick test` runs. Each
//! top-level block is a case of the module's `tests`, in the order written.

mod code;
mod editor;
mod fixtures;
mod layout;
mod read;
mod rows;
mod state;

use flick_plugin::{
    Arena, Block, Compiler, Diagnostic, Fact, HeadSpec, Highlight, Hover, Piece, Plugin,
    PluginMetadata,
};

use crate::read::Bodies;

pub struct Test;

/// Static metadata declared by the first-party test plugin.
#[must_use]
pub fn metadata() -> PluginMetadata {
    PluginMetadata {
        heads: vec![
            HeadSpec {
                head: "suite".into(),
                module: "Test".into(),
                top: true,
                bracket_args: true,
                ..HeadSpec::default()
            },
            HeadSpec {
                head: "test".into(),
                module: "Test".into(),
                top: true,
                bracket_args: true,
                ..HeadSpec::default()
            },
        ],
        watch: Vec::new(),
        derives: Vec::new(),
        own_codes: Vec::new(),
    }
}

impl Plugin for Test {
    fn metadata(&self) -> PluginMetadata {
        metadata()
    }
    fn collect(&self, blocks: &[Block], _: &mut dyn Compiler) -> Vec<u8> {
        state::collect(blocks)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let item = read::block(block, compiler, Bodies::Lenient)?;
        let (index, count, named) = state::place(state, block);
        if named {
            return Err(vec![read::duplicate(block, item.name)]);
        }
        Ok(code::arena(block, &item, index, count))
    }

    fn hover(&self, block: &Block, _: &[u8], offset: u32, _: &[Fact]) -> Option<Hover> {
        editor::hover(block, offset)
    }

    fn highlight(&self, block: &Block, _: &[u8]) -> Vec<Highlight> {
        editor::highlight(block)
    }

    fn format(&self, block: &Block, _: &[u8], compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
        layout::format(block, compiler)
    }
}

/// Whether `head` is one of this plugin's.
#[must_use]
pub fn owns(head: &str) -> bool {
    metadata().heads.iter().any(|h| h.head == head)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::fixtures::{NOT_CONCURRENT, UNAVAILABLE};
    use crate::read::{DUPLICATE, MALFORMED};

    #[test]
    fn std_explains_every_code_of_the_plugin() {
        let explain = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/std/explain");
        for code in [MALFORMED, DUPLICATE, UNAVAILABLE, NOT_CONCURRENT] {
            let path = explain.join(format!("{code}.md"));
            assert!(path.exists(), "no {}", path.display());
        }
    }
}
