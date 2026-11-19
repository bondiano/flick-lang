//! `if c then a else b`, whose branches in braces hold statements.

use super::expr::{expr, stmt};
use super::{Done, K, Parser};

pub(super) fn if_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    expr(p);
    if p.expect(K::ThenKw) {
        branch(p);
        // without `else` the `if` is `Unit`, which the checker requires of `then`
        if p.eat(K::ElseKw) {
            if p.at(K::IfKw) && !p.raw().line_start {
                if_expr(p);
            } else {
                branch(p);
            }
        }
    }
    p.complete(m, K::IfExpr)
}

/// A branch of an `if`: braces hold its statements, as after `do`; else a
/// block on the next lines or one statement on the line.
fn branch(p: &mut Parser) {
    if !p.at(K::LBrace) {
        p.body(K::Block, stmt);
    } else if record_update_ahead(p) {
        // `then { r with f = v }`, a record before braces held statements:
        // FLICK-T074 asks for its parentheses
        let m = p.start();
        expr(p);
        p.complete(m, K::Block);
    } else {
        p.braces(K::Block, stmt, |_| false);
    }
}

/// Whether the braces ahead start `e with`, which no statement does; the
/// `with` of a `match` or `try` belongs to that.
fn record_update_ahead(p: &Parser) -> bool {
    let mut depth = 0;
    for i in p.pos + 1..p.toks.len() {
        let kind = p.toks[i].kind;
        match kind {
            K::LParen | K::LBracket | K::LBrace => depth += 1,
            K::RParen | K::RBracket | K::RBrace if depth > 0 => depth -= 1,
            K::RBrace => return false,
            K::WithKw if depth == 0 => return true,
            K::Eq | K::Semi | K::MatchKw | K::TryKw if depth == 0 => return false,
            _ if depth == 0 && p.sep_at(i).is_some() => return false,
            _ => {}
        }
    }
    false
}
