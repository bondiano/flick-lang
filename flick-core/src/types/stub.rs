//! A fix for a call of a function that does not exist yet: its definition.

use super::Checker;
use super::ty::Type;
use crate::diagnostics::{Edit, Fix, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Gives the "cannot find value" reported for `head` since diagnostic
    /// `since` a fix defining it after the current declaration, with
    /// parameters of the types of `args` and a `todo` body.
    pub(super) fn stub_fix(&mut self, since: usize, head: &Node, args: &[&Node], types: &[Type]) {
        let span = self.span(head);
        let name = self.text_of(head);
        let Some(end) = self.current_end() else {
            return;
        };
        let Some(i) = self.diagnostics[since..].iter().position(|d| {
            d.code == codes::T001
                && d.span == Some(span)
                && d.message.starts_with("cannot find value")
        }) else {
            return;
        };
        // `schema T`, as a type's witness was written before `T.schema`
        if let (Some(ty), "schema") = (args.first().filter(|a| self.is_type_path(a)), name.as_str())
        {
            let (start, end) = (span.start, self.span(ty).end);
            let replacement = format!("{}.schema", self.text_of(ty));
            let d = &mut self.diagnostics[since + i];
            d.fixes.push(Fix {
                safe: true,
                message: format!("write `{replacement}`"),
                edits: vec![Edit {
                    span: Span::new(self.file, start, end),
                    replacement,
                }],
            });
            return;
        }
        let mut params: Vec<String> = Vec::new();
        for (n, arg) in args.iter().enumerate() {
            let named = (arg.kind == K::NameExpr)
                .then(|| self.text_of(arg))
                .filter(|a| !a.contains('.') && a.starts_with(|c: char| c.is_ascii_lowercase()));
            let param = named.unwrap_or_else(|| format!("arg{}", n + 1));
            params.push(if params.contains(&param) {
                format!("{param}{}", n + 1)
            } else {
                param
            });
        }
        let mut signature: Vec<String> = types
            .iter()
            .map(|t| {
                let shown = self.show(t);
                if shown.contains("->") {
                    format!("({shown})")
                } else {
                    shown
                }
            })
            .collect();
        signature.push("_".to_owned());
        let stub = format!(
            "\n\n{name} : {}\nlet {name} {} = todo",
            signature.join(" -> "),
            params.join(" ")
        );
        let d = &mut self.diagnostics[since + i];
        d.fixes.push(Fix {
            safe: false,
            message: format!("define `{name}`"),
            edits: vec![Edit {
                span: Span::new(self.file, end, end),
                replacement: stub,
            }],
        });
    }
}

impl Checker<'_> {
    /// Whether `node` names a type, maybe qualified: `Profile`, `App.Profile`.
    fn is_type_path(&self, node: &Node) -> bool {
        let text = self.text_of(node);
        node.kind == K::NameExpr
            && text
                .rsplit('.')
                .next()
                .is_some_and(|last| last.starts_with(|c: char| c.is_ascii_uppercase()))
    }
}

#[cfg(test)]
mod tests {
    use crate::source::SourceMap;

    /// `text` after the first fix of the diagnostic with `code`.
    fn fixed(body: &str, code: &str) -> String {
        let text = format!(
            "module Probe exposing (..)\n\ntype Tri = A | B Float | C Float Float\n\n{body}\n"
        );
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text.clone());
        let diagnostics = crate::check(&sources, file);
        let d = diagnostics
            .iter()
            .find(|d| d.code.to_string() == code)
            .unwrap_or_else(|| panic!("no {code}: {diagnostics:?}"));
        let mut out = text;
        for e in d.fixes[0].edits.iter().rev() {
            out.replace_range(e.span.start as usize..e.span.end as usize, &e.replacement);
        }
        out
    }

    #[test]
    fn a_missing_function_is_defined_after_its_caller() {
        let out = fixed(
            "f : Float -> String\nlet f n = describe n \"x\"",
            "FLICK-T001",
        );
        assert!(
            out.ends_with("let f n = describe n \"x\"\n\ndescribe : Float -> String -> _\nlet describe n arg2 = todo\n"),
            "{out}"
        );
    }

    #[test]
    fn schema_of_a_type_is_its_witness_now() {
        let out = fixed(
            "type alias Point = { x : Float }\n  deriving Schema\n\np : Schema Point\nlet p = schema Point",
            "FLICK-T001",
        );
        assert!(out.ends_with("let p = Point.schema\n"), "{out}");
    }

    #[test]
    fn a_non_exhaustive_match_gets_the_missing_arms() {
        let out = fixed(
            "f : Tri -> Float\nlet f t =\n  match t with\n  | A -> 0",
            "FLICK-T009",
        );
        assert!(
            out.ends_with("  | A -> 0\n  | B _ -> todo\n  | C _ _ -> todo\n"),
            "{out}"
        );
    }
}
