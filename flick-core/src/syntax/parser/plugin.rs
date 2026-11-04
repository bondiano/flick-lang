//! Blocks of plugins: `import Sql exposing (plugin sql)` makes `sql` start
//! a block in the rest of the file, and `sql "x" (T) { … }` keeps its
//! arguments and body as tokens for the plugin of `Sql`'s package to read. A head only the same
//! file imports counts, so `f { a = 1 }` stays a call with a record wherever
//! no import says so.

use super::{K, Parser, SyntaxKind};

/// Whether an exposed item `plugin head` starts here.
pub(super) fn at_head(p: &Parser) -> bool {
    p.at_word("plugin") && p.nth(1) == K::Ident
}

/// `plugin head` in the exposed items of an import: `head` starts plugin
/// blocks from here on.
pub(super) fn head(p: &mut Parser) {
    p.bump_as(K::PluginKw);
    let head = p.text_of(p.raw());
    p.plugin_heads.push(head);
    p.bump();
}

/// Whether a plugin block starts here: a head an import exposed, its
/// arguments (strings and `( … )`) on the same line, then `{`.
pub(super) fn at_block(p: &Parser) -> bool {
    if p.no_brace || !p.plugin_heads.contains(&p.text_of(p.raw())) {
        return false;
    }
    let mut depth = 0u32;
    for i in p.pos + 1..p.toks.len() {
        if p.sep_at(i).is_some() {
            return false;
        }
        match p.toks[i].kind {
            K::LParen => depth += 1,
            K::RParen if depth > 0 => depth -= 1,
            K::LBrace if depth == 0 => return true,
            _ if depth > 0 => {}
            K::String => {}
            _ => return false,
        }
    }
    false
}

/// `head args { … }`, the arguments and the body kept as tokens.
pub(super) fn block(p: &mut Parser) -> SyntaxKind {
    p.bump();
    loop {
        match p.raw().kind {
            K::String => p.bump(),
            K::LParen => raw_group(p, K::LParen, K::RParen, K::PluginArgs),
            _ => break,
        }
    }
    raw_braces(p);
    K::PluginExpr
}

/// `{ … }` with the tokens between kept flat in a `Block`, braces balanced.
pub(super) fn raw_braces(p: &mut Parser) {
    raw_group(p, K::LBrace, K::RBrace, K::Block);
}

/// `open … close` with the tokens between kept flat in a node `kind`,
/// `open` and `close` balanced.
fn raw_group(p: &mut Parser, open: SyntaxKind, close: SyntaxKind, kind: SyntaxKind) {
    let opener = p.raw();
    if !p.expect(open) {
        return;
    }
    let body = p.start();
    let mut depth = 0u32;
    loop {
        match p.raw().kind {
            K::Eof => break,
            k if k == close && depth == 0 => break,
            k if k == open => depth += 1,
            k if k == close => depth -= 1,
            _ => {}
        }
        p.bump();
    }
    p.complete(body, kind);
    p.expect_closer(close, opener);
}
