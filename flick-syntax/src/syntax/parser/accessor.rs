//! Paths of fields: `user.address.city`, and accessors `.address.city` as
//! values of their own. A step written `.?label` is optional: it reads the
//! field of the `Option` before it, and its `?` stays in the step.

use super::{Done, Parser};
use crate::diagnostics::codes;
use crate::syntax::kind::SyntaxKind as K;

/// At the current `.` of an accessor: whether `?` follows it without a
/// space, making the step optional, and whether a name then follows
/// without a space. A `.` right after a value was already read as its field.
fn accessor_ahead(p: &Parser) -> (bool, bool) {
    let dot = p.raw();
    let at = |i: usize| p.toks[(p.pos + i).min(p.toks.len() - 1)];
    let next = at(1);
    let optional = dot.kind == K::Dot && next.kind == K::Question && next.start == dot.end;
    let (label, after) = if optional {
        (at(2), next.end)
    } else {
        (next, dot.end)
    };
    let named = dot.kind == K::Dot && label.kind == K::Ident && label.start == after;
    (optional, named)
}

/// `.address.city`: the steps of a path of fields, written without spaces.
/// A step whose label is missing, as in `.address. users`, ends the path:
/// the words after it are read as they would be after a whole accessor.
pub(super) fn accessor(p: &mut Parser) -> K {
    loop {
        let step = p.start();
        let dot = p.raw();
        let (optional, named) = accessor_ahead(p);
        // reported on its own dot: an error remembered at the next token
        // would hide that of the statement starting there
        if !named {
            p.error_at(
                codes::S007,
                p.span(dot.start, dot.end),
                "expected a field name right after `.`".into(),
            );
        }
        p.bump();
        if optional {
            p.bump();
        }
        if named {
            p.bump();
        }
        p.complete(step, K::AccessorStep);
        if !(named && p.at(K::Dot) && p.adjacent()) {
            return K::AccessorExpr;
        }
    }
}

/// `lhs` followed by `.field` and `.?field` accesses and `?`, written
/// without spaces.
pub(super) fn accesses(p: &mut Parser, mut lhs: Done) -> Option<Done> {
    loop {
        if p.at(K::Dot) && p.adjacent() {
            let m = p.precede(lhs);
            p.bump();
            if p.at(K::Question) && p.adjacent() {
                p.bump();
            }
            if p.at_record_label() && p.adjacent() {
                p.bump_as(K::Ident);
            } else {
                p.expect(K::Ident);
            }
            lhs = p.complete(m, K::FieldExpr);
        } else if p.at(K::Question) && p.adjacent() {
            let m = p.precede(lhs);
            p.bump();
            lhs = p.complete(m, K::QuestionExpr);
        } else {
            return Some(lhs);
        }
    }
}
