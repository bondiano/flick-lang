//! Blocks of plugins: `import Pairs exposing (plugin pairs)` makes `pairs`
//! start a block in the rest of the file. `sql p { … }`, `sql "x" (T) { … }` and
//! `server App.Api.place` are expressions; at the start of a declaration,
//! `sql table Name "t" { … }` and `server fn ping x = …` are declarations.
//! The arguments, the body and the rest of a declaration are kept as tokens
//! for the plugin of `Sql`'s package to read. A head only the same
//! file imports counts, so `f { a = 1 }` stays a call with a record wherever
//! no import says so; implicit heads in the parser's [`HeadCatalog`] need no
//! import where they start a block, before `{` or as a declaration, and stay
//! names elsewhere.

use super::{K, Parser, SyntaxKind};
use crate::syntax::heads::SIBLINGS;

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
/// arguments on the same line, then `{ … }` or not; or an implicit head from
/// the catalog with its arguments and `{ … }`, so that a value of that name
/// elsewhere stays one, or before a name for a `bare` head.
pub(super) fn at_block(p: &Parser) -> bool {
    let head = p.text_of(p.raw());
    let rule = p.catalog.get(head);
    let imported = p.plugin_heads.contains(&head);
    let implicit = rule.is_some_and(|r| r.implicit);
    if !imported && !implicit {
        return false;
    }
    if rule.is_some_and(|r| r.top && (!r.implicit || !imported)) {
        return false;
    }
    let bare_implicit =
        implicit && rule.is_some_and(|r| r.bare) && matches!(p.nth(1), K::Ident | K::Con);
    let allow_bodiless = match rule {
        Some(r) if r.implicit => imported || bare_implicit,
        Some(r) => imported && r.bare,
        None => imported,
    };
    let brackets = rule.is_some_and(|r| r.bracket_args);
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
            K::LBracket if brackets => depth += 1,
            K::RBracket if brackets && depth > 0 => {
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
    depth == 0 && arguments > 0 && allow_bodiless
}

/// Whether the current head's line reaches `{` across balanced head arguments.
fn has_brace_on_line(p: &Parser, brackets: bool) -> bool {
    let mut depth = 0u32;
    for i in p.pos + 1..p.toks.len() {
        if p.sep_at(i).is_some() {
            break;
        }
        match p.toks[i].kind {
            K::LParen => depth += 1,
            K::RParen if depth > 0 => depth -= 1,
            K::LBracket if brackets => depth += 1,
            K::RBracket if brackets && depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            K::LBrace => return true,
            k if is_argument(k) => {}
            _ => break,
        }
    }
    false
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
    let brackets = p.catalog.bracket_args(p.text_of(p.raw()));
    p.bump();
    arguments(p, brackets);
    if p.at(K::LBrace) && !p.no_brace {
        raw_braces(p);
    }
    K::PluginExpr
}

/// The arguments of a head on its line, `( … )`, and `[ … ]` when
/// `brackets`, kept as tokens.
fn arguments(p: &mut Parser, brackets: bool) {
    loop {
        match p.current() {
            K::LParen => raw_group(p, K::LParen, K::RParen, K::PluginArgs),
            K::LBracket if brackets => raw_group(p, K::LBracket, K::RBracket, K::PluginArgs),
            k if is_argument(k) => p.bump(),
            _ => break,
        }
    }
}

/// Whether a declaration of a plugin starts here: a head an import exposed
/// at the start of a declaration, not its signature; or a catalog head
/// matching its `declarations`, `named`, or `top` classification.
pub(super) fn at_declaration(p: &Parser) -> bool {
    if p.nth(1) == K::Colon {
        return false;
    }
    let head = p.text_of(p.raw());
    let imported = p.plugin_heads.contains(&head);
    let Some(rule) = p.catalog.get(head) else {
        return imported;
    };
    let classified = rule.top
        || (rule.named && p.nth(1) == K::Ident)
        || (rule.declarations.iter()).any(|w| super::decl::word_at(p, 1, w));
    if rule.implicit {
        imported || classified
    } else {
        if !imported {
            return false;
        }
        if rule.top || rule.named || !rule.declarations.is_empty() {
            classified || has_brace_on_line(p, rule.bracket_args)
        } else {
            true
        }
    }
}

/// `head args { … }`, or `head args` and the rest of the declaration up to
/// the next one, all kept as tokens.
pub(super) fn declaration(p: &mut Parser) {
    let m = p.start();
    let brackets = p.catalog.bracket_args(p.text_of(p.raw()));
    p.bump();
    arguments(p, brackets);
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

/// `open$module M [Child] [M.through] [siblings$by M.siblings] (body)`, the
/// form generated code opens a module with: its path, then the type of a
/// child, when calls of the body give their last list element by element,
/// then the function a value is applied through, a path that ends in a
/// lowercase name, then the function of the list of the body's lines.
pub(super) fn opened(p: &mut Parser) -> SyntaxKind {
    p.bump();
    super::expr::con_path(p);
    while p.at(K::Con) {
        let named = p.start();
        super::expr::con_path(p);
        let function = p.toks[p.pos - 1].kind == K::Ident;
        p.complete(named, if function { K::NameExpr } else { K::NameType });
        if function {
            break;
        }
    }
    if p.at_word(SIBLINGS) {
        p.bump();
        let named = p.start();
        super::expr::con_path(p);
        p.complete(named, K::NameExpr);
    }
    super::expr::atom(p);
    K::OpenExpr
}
