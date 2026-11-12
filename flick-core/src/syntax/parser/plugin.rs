//! Blocks of plugins: `import Pairs exposing (plugin pairs)` makes `pairs`
//! start a block in the rest of the file. `sql p { … }`, `sql "x" (T) { … }` and
//! `server App.Api.place` are expressions; at the start of a declaration,
//! `sql table Name "t" { … }` and `server fn ping x = …` are declarations.
//! The arguments, the body and the rest of a declaration are kept as tokens
//! for the plugin of `Sql`'s package to read. A head only the same
//! file imports counts, so `f { a = 1 }` stays a call with a record wherever
//! no import says so; the heads of std's plugin ([`HEADS`]) need no import
//! where they start a block, before `{` or as a declaration, and stay names
//! elsewhere.

use super::{K, Parser, SyntaxKind};
use crate::syntax::heads::{BARE, DECLARATIONS, HEADS, NAMED};

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

/// Whether a plugin block starts here: a head an import exposed with its
/// arguments on the same line, then `{ … }` or not; or one of the
/// compiler's own heads with its arguments and `{ … }`, so that a value of
/// that name elsewhere stays one, or before a name for one of [`BARE`].
pub(super) fn at_block(p: &Parser) -> bool {
    let head = p.text_of(p.raw());
    let imported = p.plugin_heads.contains(&head)
        || BARE.contains(&head) && matches!(p.nth(1), K::Ident | K::Con);
    if !imported && !HEADS.iter().any(|(h, _)| *h == head) {
        return false;
    }
    let mut depth = 0u32;
    let mut arguments = 0usize;
    for i in p.pos + 1..p.toks.len() {
        if p.sep_at(i).is_some() {
            break;
        }
        match p.toks[i].kind {
            K::LParen => depth += 1,
            K::RParen if depth > 0 => {
                depth -= 1;
                arguments += usize::from(depth == 0);
            }
            _ if depth > 0 => {}
            // where `{` ends the head, as after `provide`, it is not the body
            K::LBrace if !p.no_brace => return true,
            k if is_argument(k) => arguments += 1,
            _ => break,
        }
    }
    depth == 0 && arguments > 0 && imported
}

/// A word, a name, a number or a string: an argument of a head, which
/// `( … )` and paths `M.x` are too.
fn is_argument(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        K::Ident | K::Con | K::Number | K::BigInt | K::String | K::Dot
    )
}

/// `head args { … }` or `head args`, the arguments and the body kept as
/// tokens.
pub(super) fn block(p: &mut Parser) -> SyntaxKind {
    p.bump();
    arguments(p);
    if p.at(K::LBrace) && !p.no_brace {
        raw_braces(p);
    }
    K::PluginExpr
}

/// The arguments of a head on its line, `( … )` kept as tokens.
fn arguments(p: &mut Parser) {
    loop {
        match p.current() {
            K::LParen => raw_group(p, K::LParen, K::RParen, K::PluginArgs),
            k if is_argument(k) => p.bump(),
            _ => break,
        }
    }
}

/// Whether a declaration of a plugin starts here: a head an import exposed
/// at the start of a declaration, not its signature; one of
/// [`DECLARATIONS`] before its word; or one of [`NAMED`] before a name.
pub(super) fn at_declaration(p: &Parser) -> bool {
    let head = p.text_of(p.raw());
    let own = (DECLARATIONS.iter()).any(|(h, w)| *h == head && super::decl::word_at(p, 1, w))
        || NAMED.contains(&head) && p.nth(1) == K::Ident;
    (p.plugin_heads.contains(&head) || own) && p.nth(1) != K::Colon
}

/// `head args { … }`, or `head args` and the rest of the declaration up to
/// the next one, all kept as tokens.
pub(super) fn declaration(p: &mut Parser) {
    let m = p.start();
    p.bump();
    arguments(p);
    if p.at(K::LBrace) {
        raw_braces(p);
        if !at_end(p) {
            p.error_expected("the end of the declaration after `}`");
        }
    }
    if !at_end(p) {
        let rest = p.start();
        while !at_end(p) {
            p.bump();
        }
        p.complete(rest, K::PluginRest);
    }
    p.complete(m, K::PluginDecl);
}

/// Whether the declaration ends here: the next one starts, or the file.
fn at_end(p: &Parser) -> bool {
    matches!(p.current(), K::LayoutSep | K::Eof)
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
