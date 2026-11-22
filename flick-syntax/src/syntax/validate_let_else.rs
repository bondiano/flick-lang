//! Validation for `let pattern = initializer else fallback`.

use super::cst::Node;
use super::kind::SyntaxKind as K;
use super::validate::Place;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

/// `let pattern = initializer else fallback`: a statement of an ordinary
/// block, without modifiers, parameters, whole-binding annotations or
/// contracts.
pub(super) fn check(
    file: FileId,
    decl: &Node,
    block: bool,
    place: Place,
    errors: &mut Vec<Diagnostic>,
) {
    let Some(else_kw) = decl.child_tokens().find(|t| t.kind == K::ElseKw) else {
        return;
    };
    let mut report = |start: u32, end: u32, message: &str| {
        errors.push(Diagnostic::error(
            codes::S007,
            message.to_owned(),
            Span::new(file, start, end),
        ));
    };
    if !block {
        report(
            else_kw.start,
            else_kw.end,
            "`let ... else` is only allowed inside a block, not as a module declaration",
        );
    } else if place == Place::Do {
        report(
            else_kw.start,
            else_kw.end,
            "`let ... else` is not allowed directly in a `do` block: use `match` instead",
        );
    } else if place == Place::For {
        report(
            else_kw.start,
            else_kw.end,
            "`let ... else` is not allowed in `for` clauses: use it in the body or use `match`",
        );
    }
    if let Some(kw) = decl.child_tokens().find(|t| t.kind == K::RecKw) {
        report(kw.start, kw.end, "`let ... else` cannot use `rec`");
    }
    if let Some(kw) = decl.child_tokens().find(|t| t.kind == K::MutKw) {
        report(kw.start, kw.end, "`let ... else` cannot use `mut`");
    }
    if let Some(kw) = decl.child_tokens().find(|t| t.kind == K::LazyKw) {
        report(kw.start, kw.end, "`let ... else` cannot use `lazy`");
    }
    if let Some(kw) = decl.child_tokens().find(|t| t.kind == K::AssertKw) {
        report(
            kw.start,
            kw.end,
            "`let ... else` cannot be combined with `assert`",
        );
    }
    if let Some(params) = decl.nodes().find(|n| n.kind == K::Params) {
        let (start, end) = params.range();
        report(
            start,
            end,
            "`let ... else` cannot declare function parameters",
        );
    }
    if let Some(colon) = decl.child_tokens().find(|t| t.kind == K::Colon) {
        report(
            colon.start,
            colon.end,
            "`let ... else` does not take a type annotation: annotate the initializer or pattern instead",
        );
    }
    if let Some(contract) = decl.nodes().find(|n| n.kind == K::Contract) {
        let (start, end) = contract.range();
        report(start, end, "`let ... else` cannot have a `where` contract");
    }
}
