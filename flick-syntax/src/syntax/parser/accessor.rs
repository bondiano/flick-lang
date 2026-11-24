//! Accessors: `.address.city` as a value of its own.

use super::Parser;
use crate::diagnostics::codes;
use crate::syntax::kind::SyntaxKind as K;

/// Whether an accessor `.label` starts at the current `.`: a name follows it
/// without a space. A `.` right after a value was already read as its field.
pub(super) fn accessor_ahead(p: &Parser) -> bool {
    let dot = p.raw();
    let next = p.toks[(p.pos + 1).min(p.toks.len() - 1)];
    dot.kind == K::Dot && next.kind == K::Ident && next.start == dot.end
}

/// `.address.city`: the steps of a path of fields, written without spaces.
/// A step whose label is missing, as in `.address. users`, ends the path:
/// the words after it are read as they would be after a whole accessor.
pub(super) fn accessor(p: &mut Parser) -> K {
    loop {
        let step = p.start();
        let dot = p.raw();
        p.bump();
        let named = p.raw().kind == K::Ident && p.raw().start == dot.end;
        if named {
            p.bump();
        } else {
            p.error_at(
                codes::S007,
                p.span(dot.start, dot.end),
                "expected a field name right after `.`".into(),
            );
        }
        p.complete(step, K::AccessorStep);
        if !(named && p.at(K::Dot) && p.adjacent()) {
            return K::AccessorExpr;
        }
    }
}
