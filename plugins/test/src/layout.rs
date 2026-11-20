//! The layout of a block: the name, the timeout and `{`, then each case and
//! hook a line, one level in, after its `///` lines and a blank line where
//! one was; a body, laid out by the compiler, on the line of its `{` where
//! it was written there, else on the lines under its name; `{}` for an
//! empty one.

use flick_plugin::{Block, Compiler, Hole, HoleKind, Piece, Span};

use crate::read::{self, Bodies, Fixture, Item, Kind, before};

/// The pieces of `block`, or none when it is no suite or test.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let item = read::block(block, compiler, Bodies::Strict).ok()?;
    let mut out = Vec::new();
    rest(block, &item, &mut out);
    // the compiler writes the head before the pieces and the closing `}` after them
    out.pop();
    Some(out)
}

/// The pieces of `item` after its word: its modifier, its name, its
/// timeout and the braces with what is inside; of a hook, the braces; of a
/// fixture, its words, its name and its expression.
fn rest(block: &Block, item: &Item, out: &mut Vec<Piece>) {
    if let Kind::Fixture(fixture) = &item.kind {
        fixture_rest(block, item, fixture, out);
        return;
    }
    if !matches!(item.kind, Kind::Hook(_)) {
        if let Some(modifier) = item.modifier {
            text(out, ".");
            text(out, block.text_of(modifier));
            text(out, " ");
        }
        if let Some(parallel) = item.parallel {
            text(out, block.text_of(parallel));
            text(out, " ");
        }
        if let Kind::Each(each) = &item.kind {
            out.push(hole(each.rows));
            text(out, " ");
        }
        text(out, block.text_of(item.name));
        if let Kind::Each(each) = &item.kind {
            text(out, " ");
            out.push(Piece::Hole(Hole {
                kind: HoleKind::Pat,
                span: each.pattern,
            }));
        }
    }
    if let Some((word, millis)) = item.timeout {
        text(out, " ");
        text(out, block.text_of(word));
        text(out, " ");
        out.push(hole(millis));
    }
    text(out, " {");
    let code = match &item.kind {
        Kind::Test(code) | Kind::Hook(code) => *code,
        Kind::Each(each) => each.body,
        Kind::Fixture(_) | Kind::Suite(_) => None,
    };
    match (&item.kind, code) {
        (Kind::Test(_) | Kind::Hook(_) | Kind::Each(_), None) | (Kind::Fixture(_), _) => {}
        // a body written on the line of its `{` is one expression, which stays there
        (_, Some(body)) if before(block, body.start) == Some(b'{') => {
            text(out, " ");
            out.push(hole(body));
            text(out, " ");
        }
        (_, Some(body)) => {
            out.push(Piece::Indent);
            out.push(Piece::Line);
            out.push(hole(body));
            out.push(Piece::Dedent);
            out.push(Piece::Line);
        }
        (Kind::Suite(items), _) => {
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
                if inner.modifier.is_none()
                    && !matches!(inner.kind, Kind::Hook(_) | Kind::Fixture(_))
                {
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

/// ` shared concurrent name = expression` after the word `fixture`: the
/// expression on the line of `=` where it was written there, else on the
/// lines under it.
fn fixture_rest(block: &Block, item: &Item, fixture: &Fixture, out: &mut Vec<Piece>) {
    for word in [fixture.shared, fixture.concurrent].into_iter().flatten() {
        text(out, " ");
        text(out, block.text_of(word));
    }
    text(out, " ");
    text(out, block.text_of(item.name));
    text(out, " =");
    match fixture.expr {
        None => {}
        Some(expr) if before(block, expr.start) == Some(b'=') => {
            text(out, " ");
            out.push(hole(expr));
        }
        Some(expr) => {
            out.push(Piece::Indent);
            out.push(Piece::Line);
            out.push(hole(expr));
            out.push(Piece::Dedent);
        }
    }
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
