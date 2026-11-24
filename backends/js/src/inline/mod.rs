//! JavaScript inline foreign code validation with exact `.fl` span mapping.

mod expr;
mod lex;
mod parse;

use std::collections::BTreeSet;

use flick_backend_support::inline_span::map_byte_range;
use flick_ir::{Binding, ItemKind, Program, Span};

use crate::foreign_meta::ForeignDiagnostic;

/// Validate a single `foreign ... from inline "..."` JavaScript expression.
pub fn validate_binding(code: &str, span: Span, source: &str) -> Option<ForeignDiagnostic> {
    parse::parse_expression(code)
        .err()
        .map(|err| to_diagnostic(&err, span, source))
}

/// Validate a single `foreign module inline "..."` JavaScript module preamble.
pub fn validate_preamble(code: &str, span: Span, source: &str) -> Option<ForeignDiagnostic> {
    parse::parse_module(code)
        .err()
        .map(|err| to_diagnostic(&err, span, source))
}

/// Validate all inline foreign bindings and module preambles in `own_modules`.
pub fn validate_program(
    program: &Program,
    sources: &[String],
    own_modules: &BTreeSet<&str>,
) -> Vec<ForeignDiagnostic> {
    let mut diagnostics = Vec::new();
    for module in &program.modules {
        if !own_modules.contains(module.name.as_str()) {
            continue;
        }
        for preamble in &module.preambles {
            let source = sources
                .get(preamble.span.file.index() as usize)
                .map_or("", String::as_str);
            if let Some(d) = validate_preamble(&preamble.code, preamble.span, source) {
                diagnostics.push(d);
            }
        }
        for item in &module.items {
            if let ItemKind::Foreign(foreign) = &item.kind
                && let Binding::Inline(inline) = &foreign.binding
            {
                let source = sources
                    .get(inline.span.file.index() as usize)
                    .map_or("", String::as_str);
                if let Some(d) = validate_binding(&inline.code, inline.span, source) {
                    diagnostics.push(d);
                }
            }
        }
    }
    diagnostics
}

fn to_diagnostic(err: &parse::ParseError, span: Span, source: &str) -> ForeignDiagnostic {
    let mapped = if source.is_empty() {
        span
    } else {
        map_byte_range(source, span, err.start, err.end)
    };
    ForeignDiagnostic {
        code: "F011",
        message: format!("invalid inline JavaScript: {}", err.message),
        span: mapped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flick_ir::FileId;

    fn span_of(src: &str, needle: &str) -> Span {
        let start = src.find(needle).expect("needle");
        Span::new(
            FileId::new(0),
            u32::try_from(start).expect("start fits u32"),
            u32::try_from(start + needle.len()).expect("end fits u32"),
        )
    }

    fn full_span(src: &str) -> Span {
        Span::new(
            FileId::new(0),
            0,
            u32::try_from(src.len()).expect("len fits u32"),
        )
    }

    #[test]
    fn accepts_valid_inline_js_expressions_and_preambles() {
        for expr in [
            "(d) => Math.cos(d)",
            "x => x + 1",
            "async (url) => (await fetch(url)).text()",
            "function(a, b) { const sum = a + b; return `sum=${sum}`; }",
            "Math.cos",
            "({ a: 1, b: [1, 2, 3], m(x) { return /a\\/b/gi.test(x); } })",
            "/* lead */ (x) => x // trail",
        ] {
            let src = format!("\"{}\"", expr.replace('\\', "\\\\").replace('"', "\\\""));
            let span = full_span(&src);
            assert_eq!(validate_binding(expr, span, &src), None, "{expr}");
        }
        let preamble = "const K = 2;\nexport function helper(x) { return x * K; }\n";
        let src = format!("\"\"\"\n{preamble}\"\"\"");
        let span = full_span(&src);
        assert_eq!(validate_preamble(preamble, span, &src), None);
    }

    #[test]
    fn maps_multiline_js_syntax_errors_to_the_exact_source_line() {
        let lit = "\"\"\"\n  const ok = 1;\n  const bad = (1 + );\n  \"\"\"";
        let src = format!("foreign module inline {lit}\n");
        let span = span_of(&src, lit);
        let code = "const ok = 1;\nconst bad = (1 + );\n";
        let diag = validate_preamble(code, span, &src).expect("syntax error");
        assert_eq!(diag.code, "F011");
        let line = src[..diag.span.start as usize]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1;
        assert_eq!(
            line,
            3,
            "span={:?}, slice={:?}",
            diag.span,
            &src[diag.span.range()]
        );
        assert_eq!(&src[diag.span.range()], ")");
    }

    #[test]
    fn rejects_trailing_semicolon_and_empty_inline_bindings() {
        let src = "\"(x) => x;\"";
        let span = full_span(src);
        let diag = validate_binding("(x) => x;", span, src).expect("trailing semi");
        assert_eq!(diag.code, "F011");
        assert_eq!(&src[diag.span.range()], ";");

        let empty = "\"\"";
        let span = Span::new(FileId::new(0), 0, 2);
        assert!(validate_binding("", span, empty).is_some());
    }

    #[test]
    fn handles_multibyte_utf8_escapes_without_panicking() {
        for valid in ["\"\\π\"", "`\\π`", "/\\π/"] {
            let src = format!("\"{}\"", valid.replace('\\', "\\\\").replace('"', "\\\""));
            assert_eq!(
                validate_binding(valid, full_span(&src), &src),
                None,
                "{valid}"
            );
        }
        for invalid in ["\"\\π", "`\\π", "/\\π"] {
            let src = format!("\"{}\"", invalid.replace('\\', "\\\\").replace('"', "\\\""));
            let diag = validate_binding(invalid, full_span(&src), &src).expect("unterminated");
            assert_eq!(diag.code, "F011");
        }
    }
}
