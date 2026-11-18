//! The layout of a block: the name and `{`, then each case a line, one
//! level in, after its `///` lines and a blank line where one was; a test's
//! body, laid out by the compiler, on the line of its `{` where it was
//! written there, else on the lines under its name; `{}` for an empty one.

use flick_plugin::{Block, Compiler, Hole, HoleKind, Piece, Span};

use crate::read::{self, Bodies, Item, Kind, before};

/// The pieces of `block`, or none when it is no suite or test.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let item = read::block(block, compiler, Bodies::Strict).ok()?;
    let mut out = Vec::new();
    rest(block, &item, &mut out);
    // the compiler writes the head before the pieces and the closing `}` after them
    out.pop();
    Some(out)
}

/// The pieces of `item` after its word: its modifier, its name and the
/// braces with what is inside.
fn rest(block: &Block, item: &Item, out: &mut Vec<Piece>) {
    if let Some(modifier) = item.modifier {
        text(out, ".");
        text(out, block.text_of(modifier));
        text(out, " ");
    }
    text(out, block.text_of(item.name));
    text(out, " {");
    match &item.kind {
        Kind::Test(None) => {}
        // a body written on the line of its `{` is one expression, which stays there
        Kind::Test(Some(body)) if before(block, body.start) == Some(b'{') => {
            text(out, " ");
            out.push(hole(*body));
            text(out, " ");
        }
        Kind::Test(Some(body)) => {
            out.push(Piece::Indent);
            out.push(Piece::Line);
            out.push(hole(*body));
            out.push(Piece::Dedent);
            out.push(Piece::Line);
        }
        Kind::Suite(items) => {
            out.push(Piece::Indent);
            let mut after = None;
            for inner in items {
                let Some(word) = inner.word else { continue };
                out.push(Piece::Line);
                // a blank line between cases stays
                let gap = after.map(|end: u32| &block.text[end as usize..word.start as usize]);
                if gap.is_some_and(blank) {
                    out.push(Piece::Line);
                }
                for line in block.docs_above(word.start, "\n").lines() {
                    text(out, if line.is_empty() { "///" } else { "/// " });
                    text(out, line);
                    out.push(Piece::Line);
                }
                text(out, block.text_of(word));
                if inner.modifier.is_none() {
                    text(out, " ");
                }
                rest(block, inner, out);
                after = Some(inner.end);
            }
            out.push(Piece::Dedent);
            out.push(Piece::Line);
        }
    }
    text(out, "}");
}

fn text(out: &mut Vec<Piece>, text: &str) {
    out.push(Piece::Text(text.to_owned()));
}

fn hole(span: Span) -> Piece {
    Piece::Hole(Hole {
        kind: HoleKind::Expr,
        span,
    })
}

/// Whether `gap`, the text between two cases, holds an empty line: one
/// after the line the first ends on and before the line the second starts.
fn blank(gap: &str) -> bool {
    let mut lines = gap.split('\n').skip(1).peekable();
    while let Some(line) = lines.next() {
        if lines.peek().is_some() && line.trim().is_empty() {
            return true;
        }
    }
    false
}
