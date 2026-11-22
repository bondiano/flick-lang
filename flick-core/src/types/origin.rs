//! The other side of a type mismatch: where the expected type came from.

use super::Checker;
use super::generalize::metas_of;
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
        match (named.as_deref(), signature) {
            (Some(name), Some(sig)) => self.explain_since(since, param, sig, &|t| {
                format!("the {nth} argument of `{name}` is declared as `{t}` here")
            }),
            (Some(name), None) => self.explain_since(since, param, span, &|t| {
                format!("`{name}` takes `{t}` as its {nth} argument")
            }),
            (None, _) => self.explain_since(since, param, span, &|t| {
                format!("this function takes `{t}` as its {nth} argument")
            }),
        }
        if let Some(name) = named.as_deref() {
            self.explain_kept_operand(since, name, index, param);
        }
    }

    /// Explains mismatches of argument `index` of the local function `name`
    /// since diagnostic `since` at the operator that kept its parameter of
    /// one type.
    fn explain_kept_operand(&mut self, since: usize, name: &str, index: usize, param: &Type) {
        if self.diagnostics.len() == since || self.cx.kept_operands.is_empty() {
            return;
        }
        let Some(local) = self.cx.locals.iter().rfind(|l| l.name == name) else {
            return;
        };
        let metas = nth_param(&local.ty, index).map_or_else(Vec::new, metas_of);
        let kept = (self.cx.kept_operands.iter()).find(|(m, _, _)| metas.contains(m));
        let Some(&(_, span, op)) = kept else {
            return;
        };
        self.explain_since(since, param, span, &|t| {
            format!("an operand of `{op}` keeps one type, so `{name}` takes only `{t}`")
        });
    }
}

/// The parameter of stage `index` (from 0) of the function `ty`.
fn nth_param(mut ty: &Type, mut index: usize) -> Option<&Type> {
    loop {
        match ty {
            Type::Forall { body, .. }
            | Type::Captured { ty: body, .. }
            | Type::Bounded { ty: body, .. } => ty = body,
            Type::Fun { param, .. } if index == 0 => return Some(param),
            Type::Fun { result, .. } => {
                index -= 1;
                ty = result;
            }
            _ => return None,
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
    use crate::diagnostics::{Related, codes};
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

    /// An argument of a local function whose parameter an operator keeps of
    /// one type is explained at that operator too, for each argument.
    #[test]
    fn an_operator_that_keeps_a_parameter_of_one_type_is_named() {
        let found = mismatches(
            "one_type : Unit ->{} Unit\n\
             let one_type () =\n  let join a b = a ++ b\n  let _ = join \"a\" \"b\"\n  \
             let _ = join [1] [2]\n  ()",
        );
        let kept = "an operand of `++` keeps one type, so `join` takes only `String`";
        let expected = ["1st", "2nd"].map(|nth| {
            (
                "expected `String`, found `List Int`".to_owned(),
                vec![
                    format!("`join` takes `String` as its {nth} argument"),
                    kept.to_owned(),
                ],
            )
        });
        assert_eq!(found, expected);
        let found = mismatches(
            "compared : Unit ->{} Bool\n\
             let compared () =\n  let differ a b = a != b\n  differ 1 2 && differ \"a\" \"b\"",
        );
        assert!(
            (found.iter()).all(|(_, related)| related.last().is_some_and(
                |r| r == "an operand of `!=` keeps one type, so `differ` takes only `Int`"
            )),
            "{found:?}"
        );
        assert_eq!(found.len(), 2, "{found:?}");
    }

    /// The note is at the operator inside the local function.
    #[test]
    fn the_note_of_a_kept_operand_is_at_the_operator() {
        let text = "module Probe exposing (..)\n\
             one_type : Unit ->{} Unit\n\
             let one_type () =\n  let join a b = a ++ b\n  let _ = join \"a\" \"b\"\n  \
             let _ = join [1] \"c\"\n  ()\n";
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text.to_owned());
        let diagnostics = crate::check(&sources, file);
        let [t003] = &diagnostics[..] else {
            panic!("{diagnostics:?}")
        };
        let at = |r: &Related| &text[r.span.start as usize..r.span.end as usize];
        let spans: Vec<_> = t003.related.iter().map(at).collect();
        assert_eq!(spans, ["join", "a ++ b"], "{t003:?}");
    }
}
