//! The layout of a query: its tokens spaced as Flick spaces them, its line
//! breaks where the user put them, each line as far in from the first line
//! of the body as it was, in levels of two columns: so it moves with its
//! head as a whole. A query on one line stays on one; a block with comments
//! stays as written.

use flick_plugin::{Block, Piece, TokenKind};

use super::parse::{self, CLAUSES, Tok};

/// The layout of the query `block`, or none to keep it as written.
pub fn format(block: &Block) -> Option<Vec<Piece>> {
    let body = block.body().rest();
    let toks = parse::tokens(block, body);
    let binder = block.arguments().first().map(|t| block.text_of(t.span));
    parse::query(None, &toks, block.whole()).ok()?;
    // comments are the user's, which a layout of words would drop
    let gaps = toks.windows(2).map(|w| {
        block.text_of(flick_plugin::Span {
            start: w[0].span.end,
            end: w[1].span.start,
        })
    });
    if gaps.clone().any(|g| g.contains("//")) {
        return None;
    }
    let breaks: Vec<bool> = (0..toks.len())
        .map(|i| i > 0 && gap(block, &toks[i - 1], &toks[i]).contains('\n'))
        .collect();
    let mut out = Vec::new();
    let opener = binder.map_or(String::new(), |b| format!("{b} {{"));
    let first_break = toks.first().is_some_and(|t| {
        let open = block.body;
        block.text[open as usize..t.span.start as usize].contains('\n')
    });
    if !first_break && !breaks.contains(&true) {
        let line = spaced(&toks);
        let space = if toks.is_empty() { "" } else { " " };
        out.push(Piece::Text(format!("{opener}{space}{line}{space}")));
        return Some(out);
    }
    if !opener.is_empty() {
        out.push(Piece::Text(opener));
    }
    out.push(Piece::Indent);
    // each line one level in for each two columns it is in from the first
    let column = |t: &Tok| {
        let before = &block.text[..t.span.start as usize];
        before.len() - before.rfind('\n').map_or(0, |i| i + 1)
    };
    let first = toks.first().map_or(0, column);
    let mut level = 0usize;
    let mut line: Vec<&Tok> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        // `on` stays on the line of its `join`
        let glued = t.text == "on"
            && line
                .first()
                .is_some_and(|f| matches!(f.text.as_str(), "join" | "left_join"));
        if (i == 0 || breaks[i]) && !glued {
            if !line.is_empty() {
                out.push(Piece::Text(spaced_refs(&line)));
                line.clear();
            }
            let target = column(t).saturating_sub(first) / 2;
            while level < target {
                out.push(Piece::Indent);
                level += 1;
            }
            while level > target {
                out.push(Piece::Dedent);
                level -= 1;
            }
            out.push(Piece::Line);
        }
        line.push(t);
    }
    if !line.is_empty() {
        out.push(Piece::Text(spaced_refs(&line)));
    }
    while level > 0 {
        out.push(Piece::Dedent);
        level -= 1;
    }
    out.extend([Piece::Dedent, Piece::Line]);
    Some(out)
}

fn gap<'b>(block: &'b Block, a: &Tok, b: &Tok) -> &'b str {
    block.text_of(flick_plugin::Span {
        start: a.span.end,
        end: b.span.start,
    })
}

fn is_clause(t: &Tok) -> bool {
    matches!(t.kind, TokenKind::Ident | TokenKind::Keyword) && CLAUSES.contains(&t.text.as_str())
}

fn spaced(toks: &[Tok]) -> String {
    spaced_refs(&toks.iter().collect::<Vec<_>>())
}

/// The tokens of a line, spaced as Flick spaces them.
fn spaced_refs(toks: &[&Tok]) -> String {
    let mut out = String::new();
    for (i, t) in toks.iter().enumerate() {
        if i > 0 && space(toks[i - 1], t, i.checked_sub(2).map(|j| toks[j])) {
            out.push(' ');
        }
        out.push_str(&t.text);
    }
    out
}

/// Whether a space goes between `prev` and `t`; `before` is the token
/// before `prev`, which says whether a `-` is a sign.
fn space(prev: &Tok, t: &Tok, before: Option<&Tok>) -> bool {
    let tight_before = matches!(t.text.as_str(), "," | ";" | "." | ")" | "]");
    let tight_after = matches!(prev.text.as_str(), "(" | "[" | "." | "$");
    let empty = prev.text == "{" && t.text == "}";
    let sign = prev.text == "-"
        && before.is_none_or(|b| {
            b.kind == TokenKind::Punct && !matches!(b.text.as_str(), ")" | "]" | "}")
                || b.kind == TokenKind::Keyword
                || is_clause(b)
        });
    !(tight_before || tight_after || empty || sign)
}
