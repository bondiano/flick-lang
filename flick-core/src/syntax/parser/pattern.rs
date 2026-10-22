//! Patterns of `let`, `fun` parameters and match arms.

use super::{Ctx, Done, K, Parser, expr, types};

/// Alternatives `p | q | …` of a match arm or a parenthesized pattern.
pub(super) fn or_pattern(p: &mut Parser) -> Option<Done> {
    let first = pattern(p)?;
    if !p.at(K::Pipe) {
        return Some(first);
    }
    let m = p.precede(first);
    while p.eat(K::Pipe) {
        pattern(p);
    }
    Some(p.complete(m, K::OrPat))
}

/// A constructor applied to atom patterns, or an atom pattern.
pub(super) fn pattern(p: &mut Parser) -> Option<Done> {
    if !p.at(K::Con) {
        return atom_pattern(p);
    }
    let m = p.start();
    expr::con_path(p);
    while at_atom_start(p) {
        atom_pattern(p);
    }
    Some(p.complete(m, K::ConPat))
}

pub(super) fn at_atom_start(p: &Parser) -> bool {
    match p.current() {
        K::Underscore
        | K::Ident
        | K::Con
        | K::Number
        | K::BigInt
        | K::String
        | K::TrueKw
        | K::FalseKw
        | K::LParen
        | K::LBracket
        | K::LBrace => true,
        K::Minus => matches!(p.nth(1), K::Number | K::BigInt),
        _ => false,
    }
}

pub(super) fn atom_pattern(p: &mut Parser) -> Option<Done> {
    if !p.enter() {
        return None;
    }
    let m = p.start();
    let kind = match p.current() {
        K::Underscore => {
            p.bump();
            K::WildcardPat
        }
        K::Ident => {
            p.bump();
            K::VarPat
        }
        K::Con => {
            expr::con_path(p);
            K::ConPat
        }
        K::Number | K::BigInt | K::String | K::TrueKw | K::FalseKw => {
            p.bump();
            K::LitPat
        }
        K::Minus if matches!(p.nth(1), K::Number | K::BigInt) => {
            p.bump();
            p.bump();
            K::LitPat
        }
        K::LParen => paren_pattern(p),
        K::LBracket => {
            p.delimited(K::RBracket, |p| {
                or_pattern(p);
            });
            K::ListPat
        }
        K::LBrace => {
            let opener = p.raw();
            p.bump();
            if p.raw().kind != K::RBrace {
                p.push(Ctx::Aligned { col: p.col() });
                p.items(record_pattern_field, true, |_| false);
                p.pop();
            }
            p.expect_closer(K::RBrace, opener);
            K::RecordPat
        }
        _ => {
            p.error_expected("a pattern");
            K::Error
        }
    };
    p.leave();
    Some(p.complete(m, kind))
}

fn paren_pattern(p: &mut Parser) -> K {
    let opener = p.raw();
    p.bump();
    if p.raw().kind == K::RParen {
        p.bump();
        return K::UnitPat;
    }
    p.push(Ctx::Bracket);
    or_pattern(p);
    let kind = if p.eat(K::Colon) {
        types::type_(p);
        K::AnnotPat
    } else if p.at(K::Comma) {
        while p.eat(K::Comma) && !p.at(K::RParen) {
            or_pattern(p);
        }
        K::TuplePat
    } else {
        K::ParenPat
    };
    p.pop();
    p.expect_closer(K::RParen, opener);
    kind
}

/// `field = pattern`, or `field` alone to bind a variable of the same name.
fn record_pattern_field(p: &mut Parser) {
    let m = p.start();
    if p.expect(K::Ident) && p.eat(K::Eq) {
        or_pattern(p);
    }
    p.complete(m, K::RecordPatField);
}
