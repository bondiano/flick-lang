//! Indented `@native <binding> when <cond>` override clauses on `let` definitions.

use super::{Ctx, K, Parser, decl};
use crate::diagnostics::codes;
use crate::syntax::when;

/// Whether the raw current tokens are `@native` (adjacent `@` and `native`).
pub(super) fn is_native_head(p: &Parser) -> bool {
    let at = p.raw();
    at.kind == K::At
        && p.toks.get(p.pos + 1).is_some_and(|next| {
            next.kind == K::Ident && next.start == at.end && p.text_of(*next) == "native"
        })
}

/// Stops an aligned `let` body block before an aligned `@native` override clause.
pub(super) fn at_aligned_native(p: &Parser) -> bool {
    p.raw().line_start && p.col() == p.aligned_col() && is_native_head(p)
}

/// Parses one or more indented `@native <binding> when <cond>` override clauses
/// attached to a `let` declaration.
pub(super) fn overrides(p: &mut Parser) {
    if !(p.sep().is_none() && p.raw().line_start && is_native_head(p)) {
        return;
    }
    let native_col = p.col();
    while p.sep().is_none() && p.raw().line_start && p.col() == native_col && is_native_head(p) {
        override_clause(p, native_col);
    }
}

/// Parses an indented `@native when <cond>` on a `type` declaration: where
/// the condition holds the type is the backend's own, without its
/// constructors, which are its Flick representation elsewhere.
pub(super) fn type_override(p: &mut Parser) {
    if !(p.sep().is_none() && p.raw().line_start && is_native_head(p)) {
        return;
    }
    let m = p.start();
    p.bump();
    p.bump();
    p.push(Ctx::Inline {
        threshold: u32::MAX,
    });
    if p.at(K::String) || p.at_word("global") || p.at_word("inline") {
        p.error_expected("`when`: the `@native` of a type names no binding, only where it holds");
        foreign_source(p);
    }
    if p.expect(K::WhenKw) {
        when_cond(p);
    }
    p.pop();
    p.complete(m, K::NativeOverride);
}

fn override_clause(p: &mut Parser, clause_col: u32) {
    let m = p.start();
    // Consume `@` and `native` while still at the clause's line start.
    p.bump();
    p.bump();
    // Header tokens stay on this line (except inside parenthesized `when` conditions),
    // so deeper-indented `ForeignMeta` lines below are not swallowed into `when`.
    p.push(Ctx::Inline {
        threshold: u32::MAX,
    });
    foreign_source(p);
    if p.expect(K::WhenKw) {
        when_cond(p);
    }
    p.pop();
    if p.sep().is_none() && p.raw().line_start && p.col() > clause_col {
        p.body_glued(K::ForeignMeta, decl::meta_item, Parser::at_aligned_catch);
    }
    p.complete(m, K::NativeOverride);
}

/// Parses the binding target after `@native`:
/// - `"Global.name"` (global shorthand)
/// - `global "Global.name"`
/// - `"./module.mjs" "exportName"`
/// - `inline "host code"`
fn foreign_source(p: &mut Parser) {
    let m = p.start();
    if p.at_word("global") || p.at_word("inline") {
        p.bump();
        p.expect(K::String);
    } else if p.at(K::String) {
        p.bump();
        if p.at(K::String) {
            p.bump();
        }
    } else {
        p.error_expected("a native binding (`\"Global\"`, `global \"...\"`, `\"module\" \"export\"`, or `inline \"...\"`)");
    }
    p.complete(m, K::ForeignSource);
}

/// Parses an unquoted environment condition after `when` (`!`, `&&`, `||`, `( )`,
/// and tags including hyphenated names like `effect-adapter`), then verifies the
/// slice against [`when::parse`] for exact tag lexical rules.
fn when_cond(p: &mut Parser) {
    let m = p.start();
    let start = p.raw().start;
    let before = p.pos;
    parse_or(p);
    let end = p.prev_end();
    if p.pos > before && !p.failed {
        let slice = &p.text[start as usize..end as usize];
        if let Err(err) = when::parse(slice) {
            let err_start = start + u32::try_from(err.range.0).unwrap_or(0);
            let err_end = start + u32::try_from(err.range.1).unwrap_or(0);
            p.error_at(
                codes::S007,
                p.span(err_start, err_end.max(err_start)),
                format!("invalid `when` condition: {err}"),
            );
        }
    }
    p.complete(m, K::WhenCond);
}

fn parse_or(p: &mut Parser) {
    if !p.enter() {
        return;
    }
    parse_and(p);
    while p.eat(K::OrOr) {
        parse_and(p);
    }
    p.leave();
}

fn parse_and(p: &mut Parser) {
    if !p.enter() {
        return;
    }
    parse_not(p);
    while p.eat(K::AndAnd) {
        parse_not(p);
    }
    p.leave();
}

fn parse_not(p: &mut Parser) {
    if !p.enter() {
        return;
    }
    if p.eat(K::Bang) {
        parse_not(p);
    } else {
        parse_atom(p);
    }
    p.leave();
}

fn parse_atom(p: &mut Parser) {
    if p.at(K::LParen) {
        let opener = p.raw();
        p.bump();
        p.push(Ctx::Bracket);
        parse_or(p);
        p.pop();
        p.expect_closer(K::RParen, opener);
    } else if p.at(K::Ident) || p.current().is_keyword() {
        p.bump_as(K::Ident);
        while p.adjacent() && p.raw().kind == K::Minus {
            p.bump();
            if p.adjacent()
                && (p.raw().kind == K::Ident
                    || p.raw().kind == K::Number
                    || p.raw().kind.is_keyword())
            {
                if p.raw().kind.is_keyword() {
                    p.bump_as(K::Ident);
                } else {
                    p.bump();
                }
            }
        }
    } else {
        p.error_expected("an environment tag, `!`, or `(`");
    }
}
