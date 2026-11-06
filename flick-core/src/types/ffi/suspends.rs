//! A `foreign async` gives a Promise, which suspends: its row says so.

use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::lang::{self, Lang};
use crate::types::ty::Type;

impl Checker<'_> {
    /// A `foreign async` gives a Promise, which suspends: the row of its
    /// runtime arrow names `Async`.
    pub(super) fn async_needs_async(&mut self, decl: &Node, body: &Type) {
        let mut ty = body;
        let effects = loop {
            match ty {
                Type::Fun {
                    effects: Some(e), ..
                } => break e,
                Type::Fun { result, .. } => ty = result,
                _ => return,
            }
        };
        let async_ = lang::label(Lang::Async);
        let named = |row: &Type| match row {
            Type::Row(r) => r.entries.iter().any(|(l, _)| Some(l.as_str()) == async_),
            _ => false,
        };
        if named(&effects.services) {
            return;
        }
        let edits: Vec<Edit> = (effects.written.adding("Async").into_iter())
            .map(|(span, replacement)| Edit { span, replacement })
            .collect();
        let message =
            "a `foreign async` gives a Promise, which suspends: the row of its arrow needs `Async`";
        let diagnostic = Diagnostic::error(codes::F001, message, self.span(decl));
        self.diagnostics.push(if edits.is_empty() {
            diagnostic
        } else {
            diagnostic.with_safe_fix(|| ("add `Async` to the row", edits))
        });
    }
}
