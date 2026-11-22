//! `match`, `try … with`, and postfix `catch` expressions.

use super::expr::{expr, stmt};
use super::{Done, K, Parser, pattern};
use crate::diagnostics::codes;

/// The arms of a `match`, a `try` or a `catch` (`handler`): there
/// `| return p ->` is recognized as `ReturnKw` (`try` handles it, `catch`
/// rejects it with a targeted diagnostic), and `return` alone is a name.
fn arms(p: &mut Parser, handler: bool) {
    arms_with(p, handler, "a match arm `| pattern -> …`");
}

fn arms_with(p: &mut Parser, handler: bool, expected: &str) {
    if !p.at(K::Pipe) {
        p.error_expected(expected);
    }
    while p.at(K::Pipe) {
        let m = p.start();
        p.bump();
        if handler && p.at_word("return") && !matches!(p.nth(1), K::Arrow | K::WhenKw | K::Pipe) {
            p.bump_as(K::ReturnKw);
        }
        pattern::or_pattern(p);
        if p.at(K::WhenKw) {
            let guard = p.start();
            p.bump();
            expr(p);
            p.complete(guard, K::Guard);
        }
        if p.expect(K::Arrow) {
            p.body(K::Block, stmt);
        }
        p.complete(m, K::MatchArm);
    }
}

pub(super) fn catch_expr(p: &mut Parser, lhs: Done) -> Done {
    let m = p.precede(lhs);
    p.bump();
    arms_with(p, true, "a catch arm `| ErrorPattern -> …`");
    if p.at(K::CatchKw) {
        let span = p.here_span();
        p.error_at(
            codes::S007,
            span,
            "cannot chain `catch` without parentheses: write `(expr catch | …) catch | …`".into(),
        );
        p.recover();
    }
    p.complete(m, K::CatchExpr)
}

pub(super) fn match_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    expr(p);
    if p.expect(K::WithKw) {
        arms(p, false);
    }
    p.complete(m, K::MatchExpr)
}

pub(super) fn try_expr(p: &mut Parser) -> Done {
    let m = p.start();
    p.bump();
    p.body(K::Block, stmt);
    if p.expect(K::WithKw) {
        arms(p, true);
    }
    p.complete(m, K::TryExpr)
}
