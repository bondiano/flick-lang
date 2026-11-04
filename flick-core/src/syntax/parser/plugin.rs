//! Blocks of plugins: `import plugin "package" exposing (head)` makes `head`
//! start a block in the rest of the file, and `head { … }` keeps its body as
//! tokens for the plugin to read. A head only the same file imports counts,
//! so `f { a = 1 }` stays a call with a record wherever no import says so.

use super::{K, Marker, Parser, SyntaxKind};

/// `plugin "package" exposing (head, …)` after `import`, whose marker is `m`.
pub(super) fn import(p: &mut Parser, m: Marker) {
    p.bump_as(K::PluginKw);
    p.bump();
    let from = p.pos;
    if p.at(K::ExposingKw) {
        super::decl::exposing(p);
    } else {
        p.error_expected("`exposing` and the heads of the plugin");
    }
    let heads: Vec<&str> = p.toks[from..p.pos]
        .iter()
        .filter(|t| t.kind == K::Ident)
        .map(|&t| p.text_of(t))
        .collect();
    p.plugin_heads.extend(heads);
    p.complete(m, K::PluginImport);
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
