//! The header of a `test.each`, `rows "name" (pattern)`, read from its
//! tokens, and the brackets around it.

use flick_plugin::{Block, Span, Token, TokenKind};

use crate::read::range;

/// The rows, the pattern and how many parts a row has of a `test.each`.
pub type Rows = (Span, Span, usize);

/// The header of a `test.each` after `.each`, `rows "name" (pattern)`,
/// maybe followed by `timeout millis`: the name, the timeout, and the rows,
/// the pattern and how many parts a row has. The name is the last string
/// before a parenthesized pattern that ends the header or comes before
/// `timeout`.
pub fn header(block: &Block, tokens: &[Token]) -> Option<(Span, Option<(Span, Span)>, Rows)> {
    let text = |i: usize| block.text_of(tokens[i].span);
    let found = (1..tokens.len()).rev().find_map(|k| {
        if tokens[k].kind != TokenKind::String
            || tokens.get(k + 1).map(|t| block.text_of(t.span)) != Some("(")
        {
            return None;
        }
        let close = matching(block, tokens, k + 1)?;
        let after = &tokens[close + 1..];
        (after.is_empty() || block.text_of(after[0].span) == "timeout").then_some((k, close))
    })?;
    let (name, close) = found;
    let rows = range(&tokens[..name])?;
    let pattern = Span {
        start: tokens[name + 1].span.start,
        end: tokens[close].span.end,
    };
    // the parts of a row are those of the pattern's tuple: its commas outside any other brackets
    let mut depth = 0usize;
    let mut parts = 1;
    for i in name + 2..close {
        match text(i) {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            "," if depth == 0 => parts += 1,
            _ => {}
        }
    }
    let timeout = match &tokens[close + 1..] {
        [] => None,
        [word, millis @ ..] => Some((word.span, range(millis)?)),
    };
    Some((tokens[name].span, timeout, (rows, pattern, parts)))
}

/// The index of the first `{` of `tokens` outside parentheses and square
/// brackets: the one that opens a case's body, past the rows of a `test.each`.
pub fn brace(block: &Block, tokens: &[Token]) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        match block.text_of(t.span) {
            "{" if depth == 0 => return Some(i),
            "(" | "[" => depth += 1,
            ")" | "]" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// The index of the `)`, `]` or `}` that closes the bracket at `open`.
fn matching(block: &Block, tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        match block.text_of(t.span) {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}
