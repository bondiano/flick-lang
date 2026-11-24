//! Validation for `@native` overrides on `let` declarations.

use super::cst::Node;
use super::kind::SyntaxKind as K;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

pub(super) fn check(file: FileId, decl: &Node, top_level: bool, errors: &mut Vec<Diagnostic>) {
    let overrides: Vec<&Node> = decl
        .nodes()
        .filter(|n| n.kind == K::NativeOverride)
        .collect();
    if overrides.is_empty() {
        return;
    }
    let first_span = {
        let (start, end) = overrides[0].range();
        Span::new(file, start, end)
    };
    let message = if !top_level {
        "`@native` overrides are only allowed on top-level `let` definitions"
    } else if decl.has_token(K::MutKw) {
        "`let mut` cannot have `@native` overrides"
    } else if decl.has_token(K::LazyKw) {
        "`let lazy` cannot have `@native` overrides"
    } else if decl.has_token(K::AssertKw) {
        "`let assert` cannot have `@native` overrides"
    } else if decl.has_token(K::ElseKw) {
        "`let ... else` cannot have `@native` overrides"
    } else if !decl.nodes().any(|n| n.kind == K::VarPat) {
        "destructuring `let` cannot have `@native` overrides: name the function directly"
    } else if decl
        .nodes()
        .find(|n| n.kind == K::Block)
        .is_some_and(|block| block.nodes().next().is_none())
    {
        "a `let` with `@native` overrides still needs a Flick fallback body"
    } else {
        return;
    };
    errors.push(Diagnostic::error(
        codes::S014,
        message.to_owned(),
        first_span,
    ));
}
