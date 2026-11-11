//! The other side of a type mismatch: where the expected type came from.

use super::Checker;
use super::ty::Type;
use super::typing::{Ref, range};
use crate::diagnostics::{Related, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Adds `origin` as a related span to each type mismatch reported since
    /// diagnostic `since` that expected `ty`; `note` gets `ty` shown.
    pub(super) fn explain_since(
        &mut self,
        since: usize,
        ty: &Type,
        origin: Span,
        note: &dyn Fn(&str) -> String,
    ) {
        if self.diagnostics.len() == since {
            return;
        }
        let shown = self.show(ty);
        let expected = format!("expected `{shown}`,");
        for d in &mut self.diagnostics[since..] {
            if d.code == codes::T003 && d.message.starts_with(&expected) {
                d.related.push(Related {
                    span: origin,
                    message: note(&shown),
                });
            }
        }
    }

    /// Explains mismatches of argument `index` (from 0) of `head` since
    /// diagnostic `since` by the parameter type `param` it was checked against.
    pub(super) fn explain_argument(
        &mut self,
        since: usize,
        head: &Node,
        index: usize,
        param: &Type,
    ) {
        let nth = ordinal(index + 1);
        let span = self.span(head);
        let named = (head.kind == K::NameExpr).then(|| self.text_of(head));
        let signature = match self.typing.names.get(&range(span)) {
            Some(Ref::Value { module, name }) if *module == self.env.module => {
                self.signature_span(name)
            }
            _ => None,
        };
        match (named, signature) {
            (Some(name), Some(sig)) => self.explain_since(since, param, sig, &|t| {
                format!("the {nth} argument of `{name}` is declared as `{t}` here")
            }),
            (Some(name), None) => {
                let unsigned = (self.cx.locals.iter().rev())
                    .find(|l| l.name == name)
                    .is_some_and(|l| l.unsigned);
                self.explain_since(since, param, span, &|t| {
                    let takes = format!("`{name}` takes `{t}` as its {nth} argument");
                    if unsigned {
                        format!("{takes}: without a signature a local function has one type")
                    } else {
                        takes
                    }
                });
            }
            (None, _) => self.explain_since(since, param, span, &|t| {
                format!("this function takes `{t}` as its {nth} argument")
            }),
        }
    }
}

/// `1st`, `2nd`, `3rd`, `4th`, … `11th`, `21st`.
fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::codes;
    use crate::source::SourceMap;

    /// `(message, related notes)` of each type mismatch in `body`.
    fn mismatches(body: &str) -> Vec<(String, Vec<String>)> {
        let text = format!(
            "module Probe exposing (..)\n\
             add3 : Float -> Float -> Float -> Float\n\
             let add3 a b c = a + b + c\n\
             {body}\n"
        );
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text);
        crate::check(&sources, file)
            .into_iter()
            .filter(|d| d.code == codes::T003)
            .map(|d| {
                (
                    d.message,
                    d.related.into_iter().map(|r| r.message).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn mismatches_name_where_the_expected_type_came_from() {
        let cases = [
            (
                "let f b = if b then 1 else \"no\"",
                "the `then` branch has type `Int`",
            ),
            (
                "let g x = match x with\n  | 0 -> \"zero\"\n  | _ -> 1",
                "the first arm has type `String`",
            ),
            (
                "let h () = add3 1 \"two\" 3",
                "the 2nd argument of `add3` is declared as `Float` here",
            ),
            (
                "let i () = Float.abs \"x\"",
                "`Float.abs` takes `Float` as its 1st argument",
            ),
        ];
        for (body, note) in cases {
            let found = mismatches(body);
            assert!(
                found.iter().any(|(_, related)| related == &[note]),
                "{body}: {found:?}"
            );
        }
    }

    #[test]
    fn an_annotated_branch_type_is_not_blamed_on_the_other_branch() {
        let found = mismatches("f : Bool -> Float\nlet f b = if b then 1 else \"no\"");
        assert_eq!(
            found,
            [("expected `Float`, found `String`".to_owned(), Vec::new())]
        );
    }
}
