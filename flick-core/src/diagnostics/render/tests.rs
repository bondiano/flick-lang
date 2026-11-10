use super::*;
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::{SourceMap, Span};

fn sample() -> (SourceMap, Vec<Diagnostic>) {
    let mut sources = SourceMap::default();
    let main = sources.add(
        "src/App/Main.fl",
        "module App.Main exposing (main)\n\nlet greeting = \"привет\"\nlet main () =\n  Platform.print greeting\n  42\n",
    );
    let other = sources.add("src/App/Other.fl", "let x = 1\n");
    let diagnostics = vec![
        // span on a non-ASCII line: bytes 48..62 are the quoted Cyrillic literal
        Diagnostic::error(
            codes::S002,
            "unsupported expression",
            Span::new(main, 48, 62),
        )
        .with_related(Span::new(main, 37, 45), "bound here")
        .with_related(Span::new(other, 4, 5), "also declared here")
        .with_fix(|| {
            (
                "use `Platform.stdout_line`",
                vec![Edit {
                    span: Span::new(main, 88, 93),
                    replacement: "stdout_line".into(),
                }],
            )
        }),
        // multi-line span
        Diagnostic::new(
            codes::S002,
            Severity::Warning,
            "body is not a call sequence",
        )
        .with_span(Span::new(main, 63, 106))
        .with_fix(|| ("see `flick explain FLICK-S002`", vec![])),
        Diagnostic::new(
            codes::P001,
            Severity::Error,
            "cannot read `missing.fl`: not found",
        ),
    ];
    (sources, diagnostics)
}

#[test]
fn sample_spans_point_where_intended() {
    let (sources, diagnostics) = sample();
    let text = |s: Span| &sources.get(s.file).text[s.range()];
    assert_eq!(text(diagnostics[0].span.expect("has span")), "\"привет\"");
    assert_eq!(text(diagnostics[0].related[0].span), "greeting");
    assert_eq!(text(diagnostics[0].fixes[0].edits[0].span), "print");
}

#[test]
fn human_plain() {
    let (sources, diagnostics) = sample();
    insta::assert_snapshot!(render_human(&sources, &diagnostics, false));
}

#[test]
fn human_color() {
    let (sources, diagnostics) = sample();
    insta::assert_snapshot!(render_human(&sources, &diagnostics, true));
}

#[test]
fn json_ndjson() {
    let (sources, diagnostics) = sample();
    let out = render_json(&sources, &diagnostics);
    for (line, diagnostic) in out.lines().zip(&diagnostics) {
        let v: serde_json::Value = serde_json::from_str(line).expect("each line is JSON");
        assert_eq!(v["schema-version"], SCHEMA_VERSION);
        assert_eq!(v["code"], diagnostic.code.to_string());
        assert_eq!(v["message"], diagnostic.message);
        assert_eq!(
            v["related"].as_array().expect("related locations").len(),
            diagnostic.related.len()
        );
    }
    insta::assert_snapshot!(out);
}
