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

/// Extracts Go import paths declared in top-level `import` clauses of a Go source
/// file (such as a sibling foreign `.go` file), stopping at the first non-import
/// declaration so string literals inside code are never mistaken for imports.
pub fn imported_packages(src: &str) -> Vec<String> {
    let Ok(tokens) = lex::lex(src) else {
        return Vec::new();
    };
    let mut pos = 0usize;
    let skip_semi = |pos: &mut usize| {
        while tokens.get(*pos).is_some_and(|t| t.kind == lex::Kind::Semi) {
            *pos += 1;
        }
    };
    skip_semi(&mut pos);
    if tokens
        .get(pos)
        .is_some_and(|t| t.kind == lex::Kind::Ident && t.text == "package")
    {
        pos += 1;
        if tokens.get(pos).is_some_and(|t| t.kind == lex::Kind::Ident) {
            pos += 1;
        }
    }
    let mut imports = Vec::new();
    loop {
        skip_semi(&mut pos);
        let Some(tok) = tokens.get(pos) else {
            break;
        };
        if tok.kind != lex::Kind::Ident || tok.text != "import" {
            break;
        }
        pos += 1;
        if tokens.get(pos).is_some_and(|t| t.kind == lex::Kind::LParen) {
            pos += 1;
            while let Some(t) = tokens.get(pos) {
                if t.kind == lex::Kind::RParen || t.kind == lex::Kind::Eof {
                    pos += 1;
                    break;
                }
                if t.kind == lex::Kind::Semi {
                    pos += 1;
                    continue;
                }
                if let Some(path) = eat_import_spec(&tokens, &mut pos) {
                    imports.push(path);
                } else {
                    break;
                }
            }
        } else if let Some(path) = eat_import_spec(&tokens, &mut pos) {
            imports.push(path);
        } else {
            break;
        }
    }
    imports
}

fn eat_import_spec(tokens: &[lex::Token<'_>], pos: &mut usize) -> Option<String> {
    if tokens
        .get(*pos)
        .is_some_and(|t| matches!(t.kind, lex::Kind::Ident | lex::Kind::Dot))
    {
        *pos += 1;
    }
    let tok = tokens.get(*pos)?;
    if tok.kind != lex::Kind::String {
        return None;
    }
    *pos += 1;
    Some(unquote_import(tok.text))
}

fn unquote_import(raw: &str) -> String {
    if let Some(inner) = raw.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        return inner.to_owned();
    }
    let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return raw.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && let Some(next) = chars.next()
        {
            out.push(next);
        } else {
            out.push(ch);
        }
    }
    out
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

    #[test]
    fn distinguishes_named_slice_array_params_and_fields_from_generic_types() {
        let preamble = "\
type Box[T any] struct {
\tv T
}

type Bag struct {
\tdata   []byte
\tfixed  [4]byte
\ttagged []byte \"json:\\\"tagged\\\"\"
\tBox[int]
\tBox[[]byte] \"tag\"
}

func helper(xs []byte) int {
\treturn len(xs)
}

func sum(items []flickrt.Value) flickrt.Value {
\treturn items[0]
}

func fixed(buf [4]byte, mat [][]int, fns []func(int) int) byte {
\t_ = mat
\t_ = fns
\treturn buf[0]
}

func unnamed(Box[int], Box[[]byte]) int {
\treturn 0
}
";
        let src = format!("\"\"\"\n{preamble}\"\"\"");
        let span = full_span(&src);
        assert_eq!(validate_preamble(preamble, span, &src), None);

        let expr =
            "func(fb *flickrt.Fiber, items []flickrt.Value) flickrt.Value { return items[0] }";
        let expr_src = format!("\"{expr}\"");
        assert_eq!(
            validate_binding(expr, full_span(&expr_src), &expr_src),
            None
        );
    }

    #[test]
    fn rejects_multibyte_utf8_escapes_in_runes_and_strings_without_panicking() {
        let rune_code = "'\\π'";
        let rune_src = "\"'\\\\π'\"";
        let diag = validate_binding(rune_code, full_span(rune_src), rune_src)
            .expect("invalid rune escape");
        assert_eq!(diag.code, "F011");
        assert!(diag.message.contains("escape"), "{}", diag.message);

        let str_code = "\"\\π\"";
        let str_src = "\"\\\"\\\\π\\\"\"";
        let diag =
            validate_binding(str_code, full_span(str_src), str_src).expect("invalid string escape");
        assert_eq!(diag.code, "F011");
        assert!(diag.message.contains("escape"), "{}", diag.message);

        for valid in ["'π'", "'\\n'", "'\\''", "'\\u03c0'", "\"π\\n\\\"ok\\\"\""] {
            let src = format!("\"{}\"", valid.replace('\\', "\\\\").replace('"', "\\\""));
            assert_eq!(
                validate_binding(valid, full_span(&src), &src),
                None,
                "{valid}"
            );
        }
    }

    #[test]
    fn imported_packages_reads_only_top_level_import_clauses() {
        let src = r#"
// leading comment with "example.com/commented"
package main

import "fmt"
import (
	alias "example.com/acme/alpha"
	_ "example.com/acme/side"
	. `example.com/acme/raw`
)

const notAnImport = "example.com/acme/literal"

func Helper() string {
	return "example.com/acme/body"
}
"#;
        assert_eq!(
            imported_packages(src),
            vec![
                "fmt".to_owned(),
                "example.com/acme/alpha".to_owned(),
                "example.com/acme/side".to_owned(),
                "example.com/acme/raw".to_owned(),
            ]
        );
    }
}
