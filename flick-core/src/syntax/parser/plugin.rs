//! Blocks of plugins: `import Sql exposing (plugin sql)` makes `sql` start
//! a block in the rest of the file, and `sql { … }` keeps its body as
//! tokens for the plugin of `Sql`'s package to read. A head only the same
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

/// Whether a plugin block starts here: a head an import exposed, then `{`.
pub(super) fn at_block(p: &Parser) -> bool {
    !p.no_brace && p.nth(1) == K::LBrace && p.plugin_heads.contains(&p.text_of(p.raw()))
}

/// `head { … }`, the body kept as tokens.
pub(super) fn block(p: &mut Parser) -> SyntaxKind {
    p.bump();
    raw_braces(p);
    K::PluginExpr
}

/// `{ … }` with the tokens between kept flat in a `Block`, braces balanced.
pub(super) fn raw_braces(p: &mut Parser) {
    let opener = p.raw();
    if !p.expect(K::LBrace) {
        return;
    }
    let body = p.start();
    let mut depth = 0u32;
    loop {
        match p.raw().kind {
            K::Eof => break,
            K::RBrace if depth == 0 => break,
            K::LBrace => depth += 1,
            K::RBrace => depth -= 1,
            _ => {}
        }
        p.bump();
    }
    p.complete(body, K::Block);
    p.expect_closer(K::RBrace, opener);
}
