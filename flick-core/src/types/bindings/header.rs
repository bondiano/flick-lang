//! Types written in a `let` header, which belong in the signature above it.

use super::super::Checker;
use super::super::convert::is_type;
use super::LetParts;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Types written in a function's `let` header instead of its signature;
    /// `true` after a diagnostic.
    pub(super) fn header_types(&mut self, decl: &Node, parts: &LetParts, signed: bool) -> bool {
        let annotated: Vec<&Node> = parts
            .params
            .iter()
            .copied()
            .filter(|p| p.kind == K::AnnotPat)
            .collect();
        if parts.params.is_empty() || (parts.header.is_none() && annotated.is_empty()) {
            return false;
        }
        let name = parts.name.clone().unwrap_or_default();
        let mut message = format!(
            "write the types of `{name}` in its signature above the `let`, not in the `let` header"
        );
        if !signed {
            let mut types: Vec<String> = parts
                .params
                .iter()
                .map(|p| match p.kind {
                    K::AnnotPat => p
                        .nodes()
                        .find(|n| is_type(n.kind))
                        .map_or("_".to_owned(), |t| self.text_of(t)),
                    K::UnitPat => "Unit".to_owned(),
                    _ => "_".to_owned(),
                })
                .collect();
            types.push(parts.header.map_or("_".to_owned(), |t| self.text_of(t)));
            message = format!("{message}: `{name} : {}`", types.join(" -> "));
        }
        self.error(codes::T005, self.span(decl), message);
        true
    }
}
