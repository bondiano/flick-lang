//! Expression holes (`_`) in application arguments (`f a _ c`).

use super::expr::postfix;
use super::{Done, Parser};
use crate::diagnostics::codes;
use crate::syntax::kind::SyntaxKind as K;

/// Parses `_` in application argument position, enforcing at most one hole per
/// application (`FLICK-S017`) and rejecting postfix forms such as `_.field` or
/// `_?` (`FLICK-S018`).
pub(super) fn argument(p: &mut Parser, has_hole: &mut bool) -> Option<Done> {
    let cur = p.raw();
    let next = p.toks[(p.pos + 1).min(p.toks.len() - 1)];
    if next.start == cur.end && matches!(next.kind, K::Dot | K::Question) {
        return postfix(p);
    }
    let m = p.start();
    let span = p.here_span();
    if *has_hole {
        p.error_at(
            codes::S017,
            span,
            "an application can have at most one `_` hole".into(),
        );
        p.bump();
        return Some(p.complete(m, K::Error));
    }
    *has_hole = true;
    p.bump();
    Some(p.complete(m, K::HoleExpr))
}

/// Reports `_` in an expression position where an application argument cannot
/// start (`FLICK-S018`).
pub(super) fn unexpected(p: &mut Parser) -> K {
    let span = p.here_span();
    p.error_at(
        codes::S018,
        span,
        "`_` is only allowed as a direct argument of an application".into(),
    );
    p.bump();
    K::Error
}
