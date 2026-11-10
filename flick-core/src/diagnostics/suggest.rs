//! "Did you mean" suggestions: the known name closest to a misspelt one.

use strsim::osa_distance;

use super::Diagnostic;
use super::Edit;
use crate::source::Span;

/// The candidate within a third of `name`'s length in edits (at least one; a
/// swap of neighbours is one edit), the closest first and ties in name order,
/// so the answer does not depend on hash map order.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .into_iter()
        .filter(|c| *c != name)
        .map(|c| (osa_distance(name, c), c))
        .filter(|(d, _)| *d <= limit)
        .min()
        .map(|(_, c)| c)
}

impl Diagnostic {
    /// Offer `replacement` for `span` as a "did you mean" fix.
    #[must_use]
    pub fn did_you_mean(self, span: Span, replacement: impl FnOnce() -> Option<String>) -> Self {
        match replacement() {
            Some(r) => self.with_fix(|| {
                (
                    format!("did you mean `{r}`?"),
                    vec![Edit {
                        span,
                        replacement: r,
                    }],
                )
            }),
            None => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::closest;
    use crate::source::SourceMap;

    #[test]
    fn primary_errors_and_warnings_do_not_depend_on_suggestions() {
        use crate::diagnostics::{Severity, codes};
        let cases = [
            (
                "let total = 1\nf : Unit -> Int\nlet f () = totl",
                codes::T001,
                Severity::Error,
            ),
            ("f : String\nlet f = true", codes::T003, Severity::Error),
            (
                "f : Int -> Int\nlet f unused = 1",
                codes::R013,
                Severity::Warning,
            ),
        ];
        for (body, code, severity) in cases {
            let mut sources = SourceMap::default();
            let file = sources.add("Probe.fl", format!("module Probe exposing (..)\n{body}\n"));
            let diagnostics = crate::check(&sources, file);
            let diagnostic = diagnostics
                .iter()
                .find(|d| d.code == code)
                .unwrap_or_else(|| panic!("{diagnostics:?}"));
            assert_eq!(diagnostic.severity, severity);
            assert_eq!(diagnostic.span.expect("primary location").file, file);
        }
    }

    #[test]
    fn replacement_factory_runs_only_for_enabled_fixes() {
        use crate::diagnostics::{Diagnostic, codes};
        use crate::source::{FileId, Span};
        use std::cell::Cell;
        let called = Cell::new(false);
        let span = Span::new(FileId::default(), 0, 4);
        let diagnostic = Diagnostic::error(codes::T001, "cannot find value `totl`", span)
            .did_you_mean(span, || {
                called.set(true);
                Some("total".to_owned())
            });
        assert!(called.get());
        assert_eq!(diagnostic.message, "cannot find value `totl`");
        assert_eq!(diagnostic.span, Some(span));
        assert_eq!(diagnostic.fixes[0].edits[0].replacement, "total");
    }

    /// The replacement each "did you mean" fix of checking `body` offers.
    fn guesses(body: &str) -> Vec<String> {
        let text = format!(
            "module Probe exposing (..)\n\
             type alias User = {{ name : String; age : Float }}\n\
             type Tri = Tri Float Float\n\
             {body}\n"
        );
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text);
        crate::check(&sources, file)
            .iter()
            .flat_map(|d| &d.fixes)
            .filter(|f| f.message.starts_with("did you mean"))
            .flat_map(|f| f.edits.iter().map(|e| e.replacement.clone()))
            .collect()
    }

    #[test]
    fn misspelt_names_get_a_replacement() {
        let cases = [
            ("let total = 1\nlet f () = totl + 1", "total"),
            ("let f () = Float.to_strng 1", "Float.to_string"),
            ("let f () = Flaot.to_string 1", "Float.to_string"),
            ("let f (u : User) = u.agee", "age"),
            ("let f () = Tir 1 2", "Tri"),
            ("let f (x : Strng) = x", "String"),
            ("let f t = match t with\n  | Trri a b -> a", "Tri"),
        ];
        for (body, want) in cases {
            assert_eq!(guesses(body), [want], "{body}");
        }
    }

    #[test]
    fn unexported_import_items_get_a_replacement() {
        let text = "module Probe exposing (..)\nimport Int exposing (to_strng)\n";
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text.to_owned());
        let fixes: Vec<String> = crate::check(&sources, file)
            .iter()
            .flat_map(|d| &d.fixes)
            .flat_map(|f| f.edits.iter().map(|e| e.replacement.clone()))
            .collect();
        assert_eq!(fixes, ["to_string"]);
    }

    #[test]
    fn picks_the_nearest_name_within_the_limit() {
        let names = ["to_string", "to_float", "total", "map"];
        assert_eq!(closest("to_strng", names), Some("to_string"));
        assert_eq!(closest("totl", names), Some("total"));
        assert_eq!(closest("zzz", names), None);
        assert_eq!(closest("mpa", ["map", "max"]), Some("map"));
    }
}
