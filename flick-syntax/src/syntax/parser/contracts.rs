//! The contract of a function: `where`, then its entries before the
//! function's `=`, `require e` and `ensure e`, one a line or one inline.
//! Each word is a keyword only at the head of an entry; a contract is proof
//! metadata, which only `flick prove` verifies.

use super::{Parser, expr};
use crate::syntax::kind::SyntaxKind as K;

pub(super) fn contract(p: &mut Parser) {
    p.bump();
    p.body(K::Contract, entry);
}

fn entry(p: &mut Parser) {
    let (keyword, node) = if p.at_word("require") {
        (K::RequireKw, K::Requires)
    } else if p.at_word("ensure") {
        (K::EnsureKw, K::Ensures)
    } else {
        p.error_expected("`require` or `ensure` after a function's `where`");
        return;
    };
    let m = p.start();
    p.bump_as(keyword);
    expr::expr(p);
    p.complete(m, node);
}
