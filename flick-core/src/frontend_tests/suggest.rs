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
