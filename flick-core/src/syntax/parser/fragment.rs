//! User code inside a plugin block: the longest expression, type or pattern
//! that starts at a token of the body, or a declaration, parsed with the
//! layout of where it stands in the file. The compiler parses the holes a
//! plugin asks for this way, and the declarations a plugin gives.

use super::build::build;
use super::types::type_;
use super::{Ctx, Done, Parser, decl, expr, pattern};
use crate::diagnostics::Diagnostic;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::lexer::RawToken;

/// An expression or a type of the body and where it ends.
pub struct Fragment {
    pub tree: Node,
    /// The end of its last token.
    pub end: u32,
    pub diagnostics: Vec<Diagnostic>,
}

/// The longest expression `tokens`, tokens of the module text `text` from
/// some token of a block on, start with; `None` when none starts there.
pub fn parse_expr_at(text: &str, file: FileId, tokens: &[Token]) -> Option<Fragment> {
    fragment(text, file, tokens, expr::expr)
}

/// [`parse_expr_at`] for a type.
pub fn parse_type_at(text: &str, file: FileId, tokens: &[Token]) -> Option<Fragment> {
    fragment(text, file, tokens, type_)
}

/// [`parse_expr_at`] for a pattern.
pub fn parse_pat_at(text: &str, file: FileId, tokens: &[Token]) -> Option<Fragment> {
    fragment(text, file, tokens, pattern::pattern)
}

/// The declaration `tokens` start with, its tree the declaration itself.
pub fn parse_decl_at(text: &str, file: FileId, tokens: &[Token]) -> Option<Fragment> {
    let mut found = fragment(text, file, tokens, |p| Some(decl::declaration(p)))?;
    let inner = found.tree.nodes().next()?.clone();
    found.tree = inner;
    Some(found)
}

/// The declarations `tokens` hold, in a `SourceFile`.
pub fn parse_decls_at(text: &str, file: FileId, tokens: &[Token]) -> Option<Fragment> {
    fragment(text, file, tokens, |p| Some(decl::declarations(p)))
}

fn fragment(
    text: &str,
    file: FileId,
    tokens: &[Token],
    parse: fn(&mut Parser) -> Option<Done>,
) -> Option<Fragment> {
    let raw: Vec<RawToken> = tokens
        .iter()
        .skip_while(|t| t.kind.is_trivia())
        .map(|t| RawToken {
            kind: t.kind,
            start: t.start,
            end: t.end,
        })
        .collect();
    let last = raw.last()?.end;
    let mut p = Parser::new(text, file, &raw);
    place(&mut p, text, last);
    // a fragment that starts a line is a block of its own, as a body is
    if p.raw().line_start {
        p.push_aligned_here();
    } else {
        p.push(Ctx::Inline {
            threshold: p.raw().line_indent,
        });
    }
    parse(&mut p)?;
    p.pop();
    let end = p.prev_end();
    let used: Vec<RawToken> = raw.into_iter().take_while(|t| t.end <= end).collect();
    Some(Fragment {
        tree: build(&used, p.events),
        end,
        diagnostics: p.errors,
    })
}

/// Columns of the tokens on the line the fragment starts on, which
/// `Parser::new` counts from the fragment, and the end of the fragment's
/// tokens as the end of the file.
fn place(p: &mut Parser, text: &str, end: u32) {
    let first = p.toks[0].start as usize;
    let line = text[..first].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line..first];
    let column = |s: &str| u32::try_from(s.chars().count()).expect("columns fit in u32");
    let col = column(before);
    let indent = column(&before[..before.len() - before.trim_start().len()]);
    let starts_line = before.trim().is_empty();
    for (i, t) in p.toks.iter_mut().enumerate() {
        if i > 0 && t.line_start {
            break;
        }
        t.col += col;
        t.line_indent = indent;
        t.line_start = i == 0 && starts_line;
    }
    let eof = p.toks.last_mut().expect("an end of file");
    eof.start = end;
    eof.end = end;
}
