//! What a plugin makes of blocks, as Flick text with the user's code in
//! place: for the tests of plugins, and for `flick expand`.

use std::sync::Arc;

use flick_plugin::{Block, HoleKind, Place, Plugin};

use super::blocks_under;
use super::render::{DECL_PLACEHOLDER, Rendered, render};
use super::source::BlockSource;
use crate::fmt;
use crate::source::FileId;
use crate::syntax;

/// The code `rendered` is, the user's code in place of each hole (spans of
/// `frame`, the text of its block), laid out by the formatter where it can:
/// an expression, or declarations by `place`.
pub fn shown(rendered: &Rendered, frame: &str, place: Place) -> String {
    let mut code = String::new();
    let mut at = 0;
    for &(offset, hole) in &rendered.holes {
        let offset = offset as usize;
        let user = frame
            .get(hole.span.start as usize..hole.span.end as usize)
            .unwrap_or("");
        // a declaration's placeholder is a line of its own
        if hole.kind == HoleKind::Decl {
            code.push_str(&rendered.text[at..offset]);
            code.push_str(user);
            at = offset + DECL_PLACEHOLDER.len();
            continue;
        }
        // the others are `(hole)` or `(Hole)`, `offset` at the word
        let text = user.len() > 1 && user.starts_with('"') && user.ends_with('"');
        let atomic = !user.contains(char::is_whitespace) && !user.starts_with('-');
        if atomic || text && !user.contains('\n') {
            code.push_str(&rendered.text[at..offset - 1]);
            code.push_str(user);
            at = offset + 5;
        } else {
            code.push_str(&rendered.text[at..offset]);
            code.push_str(user);
            at = offset + 4;
        }
    }
    code.push_str(&rendered.text[at..]);
    if place == Place::Declaration {
        let laid_out = fmt::format(&format!("{code}\n"), FileId::default());
        return laid_out.map_or(code, |text| text.trim_end().to_owned());
    }
    let code = without_parentheses(&code);
    let laid_out = fmt::format(&format!("let it = {code}\n"), FileId::default());
    match laid_out {
        Ok(text) => {
            let text = text.strip_prefix("let it =").unwrap_or(&text);
            let lines: Vec<&str> = text.trim().lines().collect();
            // the lines after the first, one level less deep
            (lines.iter().enumerate())
                .map(|(i, l)| {
                    if i == 0 {
                        *l
                    } else {
                        l.strip_prefix("  ").unwrap_or(l)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Err(_) => code.to_owned(),
    }
}

/// `code` without the parentheses around the whole of it.
fn without_parentheses(code: &str) -> &str {
    let Some(inner) = code.strip_prefix('(').and_then(|c| c.strip_suffix(')')) else {
        return code;
    };
    let mut depth = 0i32;
    for c in inner.chars() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        // `(a) (b)`: the first closes before the end
        if depth < 0 {
            return code;
        }
    }
    inner
}

/// What `plugin` makes of each block of `source`, a module whose imports
/// expose the plugin's heads, in order: the code, as [`shown`], or each
/// error the plugin or the compiler finds, as ``code `text`: message``
/// (the plugin's own code, without its package).
pub fn expand(plugin: &impl Plugin, source: &str) -> Vec<Result<String, Vec<String>>> {
    let file = FileId::default();
    let parse = syntax::parse(source, file);
    let mut nodes = Vec::new();
    blocks_under(&parse.tree, &mut nodes);
    let text: Arc<str> = Arc::from(source);
    let mut sources: Vec<BlockSource> = (nodes.into_iter())
        .filter_map(|n| BlockSource::new(Arc::clone(&text), file, n))
        .filter(BlockSource::closed)
        .collect();
    let blocks: Vec<Block> = sources.iter().map(BlockSource::block).collect();
    let state = plugin.collect(&blocks);
    (sources.iter_mut().zip(&blocks))
        .map(|(source, block)| {
            let expanded = plugin.expand(block, &state, source);
            let mut errors: Vec<String> = (source.diagnostics.iter())
                .map(|d| {
                    let at = d.span.map_or("", |s| &source.text()[s.range()]);
                    format!("{} `{at}`: {}", d.code, d.message)
                })
                .collect();
            match expanded {
                Ok(arena) => match render(&arena, block.whole().end, block.place) {
                    Ok(rendered) if errors.is_empty() => {
                        return Ok(shown(&rendered, &block.text, block.place));
                    }
                    Ok(_) => {}
                    Err(why) => errors.push(format!("gave code that is not valid: {why}")),
                },
                Err(refused) => errors.extend(refused.iter().map(|d| {
                    let code = d.code.as_deref().unwrap_or("refused");
                    if d.span == block.whole() {
                        format!("{code} on the block: {}", d.message)
                    } else {
                        format!("{code} `{}`: {}", block.text_of(d.span), d.message)
                    }
                })),
            }
            Err(errors)
        })
        .collect()
}
