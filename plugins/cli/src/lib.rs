//! The plugin of `std` for `cli "name" { … }`: the clauses of a command line
//! stand for the pipeline of `Cli.*` calls they spell out,
//! `Cli.command "name" |> Cli.flag … |> Cli.handle f`, so the options record,
//! its rows and constraints come from the combinators' signatures. What the
//! types cannot state is checked here: names, aliases, defaults and the
//! shape of the command.

mod code;
mod editor;
mod read;
mod shape;

use flick_plugin::{
    Arena, Block, Compiler, Completion, Diagnostic, Highlight, Hover, Plugin, Span,
};

pub struct Cli;

impl Plugin for Cli {
    fn collect(&self, _: &[Block]) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let command = read::command(block, compiler)?;
        let wrong = shape::check(block, &command);
        if wrong.is_empty() {
            Ok(code::arena(block, &command))
        } else {
            Err(wrong)
        }
    }

    fn complete(&self, block: &Block, _: &[u8], offset: u32) -> Vec<Completion> {
        editor::complete(block, offset)
    }

    fn hover(&self, block: &Block, _: &[u8], offset: u32) -> Option<Hover> {
        editor::hover(block, offset)
    }

    fn highlight(&self, block: &Block, _: &[u8]) -> Vec<Highlight> {
        editor::highlight(block)
    }
}

/// The text of `span` in the block.
fn text(block: &Block, span: Span) -> &str {
    block
        .text
        .get(span.start as usize..span.end as usize)
        .unwrap_or("")
}

/// The length of the block's text.
fn len(block: &Block) -> u32 {
    u32::try_from(block.text.len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::read::MALFORMED;

    /// Every code the plugin reports, as `shape` and `read` give them.
    const CODES: [&str; 8] = [
        "L001", "L002", "L003", "L004", "L005", "L006", "L007", "L009",
    ];

    #[test]
    fn std_explains_every_code_of_the_plugin() {
        let explain = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/std/explain");
        assert_eq!(MALFORMED, CODES[7]);
        for code in CODES {
            let path = explain.join(format!("{code}.md"));
            assert!(path.exists(), "no {}", path.display());
        }
        let sources = [include_str!("shape.rs"), include_str!("read.rs")].concat();
        for n in 1..=9 {
            let code = format!("\"L00{n}\"");
            assert_eq!(
                sources.contains(&code),
                CODES.iter().any(|c| format!("\"{c}\"") == code),
                "{code}"
            );
        }
    }
}
