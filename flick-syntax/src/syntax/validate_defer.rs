//! Validation for scope-owned `defer { … }` and `errdefer { … }` statements.

use super::cst::Node;
use super::kind::SyntaxKind as K;
use super::validate::Place;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

/// `defer { … }` / `errdefer { … }` is a statement of an ordinary block,
/// never a first-class value expression.
pub(super) fn check(
    src: &str,
    file: FileId,
    parent: &Node,
    index: usize,
    child: &Node,
    place: Place,
    errors: &mut Vec<Diagnostic>,
) {
    let kw = child
        .child_tokens()
        .find(|t| matches!(t.kind, K::DeferKw | K::ErrdeferKw));
    let word = kw.map_or("defer", |t| t.text(src));
    let span = kw.map_or_else(
        || {
            let (start, end) = child.range();
            Span::new(file, start, end)
        },
        |t| Span::new(file, t.start, t.end),
    );
    let message = if parent.kind != K::Block {
        format!(
            "`{word} {{ … }}` is a statement, not an expression; if calling a function named `{word}` with a record, write `({word}) {{ … }}`"
        )
    } else if place == Place::Do {
        format!(
            "`{word} {{ … }}` cannot appear in a `do` block: its statements run in the functions `flat_map` takes"
        )
    } else if place == Place::For {
        format!(
            "`{word} {{ … }}` cannot appear in `for` clauses: write it in a `for … do` body inside a `scope`"
        )
    } else if place == Place::ValueBlock && index + 1 == parent.nodes().count() {
        format!(
            "`{word} {{ … }}` is a statement, not a value expression; if calling a function named `{word}` with a record, write `({word}) {{ … }}`"
        )
    } else {
        return;
    };
    errors.push(Diagnostic::error(codes::S007, message, span));
}
