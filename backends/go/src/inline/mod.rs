//! Go inline foreign code validation and preamble `import` hoisting with exact
//! `.fl` span mapping.

mod expr;
mod lex;
mod parse;

use std::collections::BTreeSet;

use flick_backend_support::inline_span::map_byte_range;
use flick_ir::{Binding, ItemKind, Program, Span};

use crate::foreign_meta::ForeignDiagnostic;

pub use parse::{ImportSpec, ParsedPreamble};

/// Validate a single `foreign ... from inline "..."` Go expression.
pub fn validate_binding(code: &str, span: Span, source: &str) -> Option<ForeignDiagnostic> {
    parse::parse_expression(code)
        .err()
        .map(|err| to_diagnostic(&err, span, source))
}

/// Validate a single `foreign module inline "..."` Go preamble.
pub fn validate_preamble(code: &str, span: Span, source: &str) -> Option<ForeignDiagnostic> {
    parse::parse_preamble(code)
        .err()
        .map(|err| to_diagnostic(&err, span, source))
}

/// Extract leading `import` declarations and the byte offset of the remaining
/// declarations from a validated Go preamble.
pub fn split_preamble(code: &str) -> ParsedPreamble {
    parse::parse_preamble(code).unwrap_or(ParsedPreamble {
        imports: Vec::new(),
        body_offset: 0,
    })
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
        message: format!("invalid inline Go: {}", err.message),
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
    fn accepts_valid_inline_go_expressions_and_preambles() {
        for expr in [
            "func(fb *flickrt.Fiber, x flickrt.Value) flickrt.Value { return math.Cos(x.(float64)) }",
            "goCos",
            "flickrt.Float(3.14)",
            "func(fb *flickrt.Fiber, xs flickrt.Value) flickrt.Value { m := make([]int, 0); for _, v := range m { _ = v }; return xs }",
        ] {
            let src = format!("\"{}\"", expr.replace('\\', "\\\\").replace('"', "\\\""));
            let span = full_span(&src);
            assert_eq!(validate_binding(expr, span, &src), None, "{expr}");
        }
        let preamble = "import (\n\t\"math\"\n\ts \"strings\"\n)\n\nfunc goCos(fb *flickrt.Fiber, x flickrt.Value) flickrt.Value {\n\t_ = s.ToUpper\n\treturn math.Cos(x.(float64))\n}\n";
        let src = format!("\"\"\"\n{preamble}\"\"\"");
        let span = full_span(&src);
        assert_eq!(validate_preamble(preamble, span, &src), None);
        let split = split_preamble(preamble);
        assert_eq!(
            split.imports,
            vec![
                ImportSpec {
                    alias: None,
                    path: "math".into()
                },
                ImportSpec {
                    alias: Some("s".into()),
                    path: "strings".into()
                },
            ]
        );
        assert!(preamble[split.body_offset..].starts_with("func goCos"));
    }

    #[test]
    fn maps_multiline_go_syntax_errors_to_the_exact_source_line() {
        let lit =
            "\"\"\"\n  import \"math\"\n  func bad(x int {\n    return math.Cos(x)\n  }\n  \"\"\"";
        let src = format!("foreign module inline {lit}\n");
        let span = span_of(&src, lit);
        let code = "import \"math\"\nfunc bad(x int {\n  return math.Cos(x)\n}\n";
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
        assert_eq!(&src[diag.span.range()], "{");
    }

    #[test]
    fn rejects_package_clause_and_late_imports_in_go_preambles() {
        let src = "\"package main\\nfunc f() {}\"";
        let span = full_span(src);
        let diag = validate_preamble("package main\nfunc f() {}", span, src).expect("package");
        assert_eq!(diag.code, "F011");
        assert!(diag.message.contains("package"));

        let src2 = "\"func f() {}\\nimport \\\"math\\\"\"";
        let span2 = full_span(src2);
        let diag2 =
            validate_preamble("func f() {}\nimport \"math\"", span2, src2).expect("late import");
        assert_eq!(diag2.code, "F011");
    }
}
