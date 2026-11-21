//! Missing-signature diagnostics and their deferred numeric-literal settlement.

use std::mem;

use super::{Checker, Diagnostic, Edit, Span, Type, codes};

impl Checker<'_> {
    /// A `let` without a signature: a value, whose type is inferred and
    /// generalized when it is built purely. A function here is an error.
    /// `FLICK-T004` for a function without a signature, with the inferred one
    /// as an edit when it is known.
    pub(super) fn missing_signature(&mut self, name: &str, ty: &Type, span: Span, errors: usize) {
        let incomplete = self.zonk(ty).contains(&mut |t| matches!(t, Type::Error));
        let diagnostic = Diagnostic::error(
            codes::T004,
            format!("function `{name}` needs a signature right above its `let`"),
            span,
        )
        .with_incomplete(incomplete);
        let suggestion = self.signature_of(ty);
        let shown = self.show_written(&suggestion);
        // the edit is offered only for a type inferred without errors
        let line = self.text[..span.start as usize]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let indent = &self.text[line..span.start as usize];
        let inferred = errors == self.diagnostics.len()
            && self.operators_fit()
            && !suggestion.contains(&mut |t| matches!(t, Type::Error | Type::Meta(_)));
        let at = (inferred && indent.chars().all(|c| c == ' ' || c == '\t')).then(|| {
            (
                u32::try_from(line).expect("offsets fit in u32"),
                indent.to_owned(),
            )
        });
        // a number literal's type is known once the declaration settles
        if self.has_open_number(ty) {
            self.signatures.push(PendingSignature {
                index: self.diagnostics.len(),
                name: name.to_owned(),
                ty: ty.clone(),
                span,
                at,
            });
            self.diagnostics
                .push(signature_diagnostic(diagnostic, name, &shown, span, None));
        } else {
            self.diagnostics
                .push(signature_diagnostic(diagnostic, name, &shown, span, at));
        }
    }

    /// Writes the signatures suggested before the number literals in them
    /// were settled, once they are.
    pub fn settle_signatures(&mut self) {
        for pending in mem::take(&mut self.signatures) {
            if self.has_open_number(&pending.ty) {
                self.signatures.push(pending);
                continue;
            }
            let suggestion = self.signature_of(&pending.ty);
            let shown = self.show_written(&suggestion);
            let incomplete = suggestion.contains(&mut |t| matches!(t, Type::Error));
            let diagnostic = self.diagnostics[pending.index]
                .clone()
                .with_incomplete(incomplete);
            self.diagnostics[pending.index] =
                signature_diagnostic(diagnostic, &pending.name, &shown, pending.span, pending.at);
        }
    }

    /// Whether `ty` holds the type of a number literal not settled yet.
    fn has_open_number(&self, ty: &Type) -> bool {
        self.zonk(ty).contains(&mut |t| {
            t.inference_meta_root()
                .is_some_and(|m| self.store.is_numeric(m))
        })
    }
}

/// A signature suggested for a function, to write later: the literals of
/// its type are not settled yet.
#[derive(Debug, Clone)]
pub(in crate::types) struct PendingSignature {
    index: usize,
    name: String,
    ty: Type,
    span: Span,
    at: Option<(u32, String)>,
}

/// `FLICK-T004` for function `name`, with the edit that inserts `shown` at
/// the start of the line `at` when the type is fully known.
fn signature_diagnostic(
    diagnostic: Diagnostic,
    name: &str,
    shown: &str,
    span: Span,
    at: Option<(u32, String)>,
) -> Diagnostic {
    diagnostic.with_fix(|| {
        let edits = match at {
            Some((at, indent)) => vec![Edit {
                span: Span::new(span.file, at, at),
                replacement: format!("{indent}{name} : {shown}\n"),
            }],
            None => Vec::new(),
        };
        (format!("add `{name} : {shown}`"), edits)
    })
}
