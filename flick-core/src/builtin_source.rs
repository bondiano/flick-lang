//! Build-time removal of embedded builtin documentation, never user sources.

use std::fmt;

use crate::diagnostics::Diagnostic;
use crate::source::{FileId, Span};
use crate::syntax::{kind::SyntaxKind as K, lexer};

#[derive(Debug)]
pub enum Error {
    Lexical(Vec<Diagnostic>),
    CliHelp(Span),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lexical(diagnostics) => {
                for (i, diagnostic) in diagnostics.iter().enumerate() {
                    if i != 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "{}: {}", diagnostic.code, diagnostic.message)?;
                    if let Some(span) = diagnostic.span {
                        write!(f, " (bytes {}..{})", span.start, span.end)?;
                    }
                }
                Ok(())
            }
            Self::CliHelp(span) => write!(
                f,
                "builtin `cli` at bytes {}..{} uses documentation as runtime help; preserve that help before disabling builtin-docs",
                span.start, span.end,
            ),
        }
    }
}

pub fn without_docs(text: &str) -> Result<String, Error> {
    let file = FileId::default();
    let (tokens, diagnostics) = lexer::lex(text, file);
    if !diagnostics.is_empty() {
        return Err(Error::Lexical(diagnostics));
    }
    if let Some(token) = tokens.iter().find(|token| token.kind == K::CliKw) {
        return Err(Error::CliHelp(Span::new(file, token.start, token.end)));
    }
    let mut stripped = String::with_capacity(text.len());
    let mut copied = 0;
    for token in tokens.iter().filter(|token| token.kind == K::DocComment) {
        stripped.push_str(&text[copied..token.start as usize]);
        copied = token.end as usize;
    }
    stripped.push_str(&text[copied..]);
    Ok(stripped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::codes;
    use crate::source::SourceMap;

    #[test]
    fn strips_only_doc_tokens_and_preserves_code_locations() {
        let text = concat!(
            "/// Документация 😀\r\n",
            "module Probe exposing (..)\r\n",
            "//// ordinary /// comment\r\n",
            "/* outer /// cli\r\n  /* nested /// */ end */\r\n",
            "  /// member λ\r\n",
            r#"let escaped = "é /// \" \\ \u{1F600}""#,
            "\r\n",
            "let triple = \"\"\"\r\n/// literal 😀\r\n\"\"\"\r\n",
            "let raw = r##\"/// raw \\q\r\n/// still raw\"##\r\n",
            "let cli_name = \"cli\" /// trailing help\r\n",
            "/// end",
        );
        let expected = concat!(
            "\r\n",
            "module Probe exposing (..)\r\n",
            "//// ordinary /// comment\r\n",
            "/* outer /// cli\r\n  /* nested /// */ end */\r\n",
            "  \r\n",
            r#"let escaped = "é /// \" \\ \u{1F600}""#,
            "\r\n",
            "let triple = \"\"\"\r\n/// literal 😀\r\n\"\"\"\r\n",
            "let raw = r##\"/// raw \\q\r\n/// still raw\"##\r\n",
            "let cli_name = \"cli\" \r\n",
        );
        let stripped = without_docs(text).expect("valid builtin text");
        assert_eq!(stripped, expected);
        assert_eq!(
            without_docs(&stripped).expect("stripped builtin text"),
            stripped
        );

        let mut sources = SourceMap::default();
        let before = sources.add("before.fl", text);
        let after = sources.add("after.fl", stripped.as_str());
        let (before_tokens, errors) = lexer::lex(text, before);
        assert!(errors.is_empty());
        let (after_tokens, errors) = lexer::lex(&stripped, after);
        assert!(errors.is_empty());
        let code = |tokens: Vec<lexer::RawToken>| {
            tokens
                .into_iter()
                .filter(|token| !token.kind.is_trivia())
                .collect::<Vec<_>>()
        };
        let before_tokens = code(before_tokens);
        let after_tokens = code(after_tokens);
        assert_eq!(before_tokens.len(), after_tokens.len());
        for (a, b) in before_tokens.iter().zip(after_tokens.iter()) {
            assert_eq!(a.kind, b.kind);
            assert_eq!(
                &text[a.start as usize..a.end as usize],
                &stripped[b.start as usize..b.end as usize],
            );
            for (a, b) in [(a.start, b.start), (a.end, b.end)] {
                assert_eq!(
                    sources.get(before).line_col(a),
                    sources.get(after).line_col(b)
                );
            }
        }
    }

    #[test]
    fn returns_real_lexical_diagnostics_without_stripping_first() {
        for (text, code) in [
            ("/// header\nlet x = \"\\q\"", codes::S005),
            ("/// header\nlet x = r#\"open", codes::S004),
            ("/// header\n/* open", codes::S004),
            ("/// header\n\tlet x = 1", codes::S009),
        ] {
            let Error::Lexical(actual) = without_docs(text).expect_err("invalid builtin text")
            else {
                panic!("expected real lexical diagnostics for {text:?}");
            };
            let (_, expected) = lexer::lex(text, FileId::default());
            assert_eq!(actual.len(), expected.len());
            assert_eq!(actual[0].code, code);
            for (a, b) in actual.iter().zip(expected.iter()) {
                assert_eq!((a.code, &a.message, a.span), (b.code, &b.message, b.span));
            }
        }
    }

    #[test]
    fn rejects_cli_help_but_not_cli_in_literals_or_comments() {
        let text = "/// help\nlet command = cli \"demo\" {}";
        let Error::CliHelp(span) = without_docs(text).expect_err("semantic CLI documentation")
        else {
            panic!("builtin CLI help must not be silently removed");
        };
        assert_eq!(&text[span.range()], "cli");
        let text = "let word = \"cli ///\"\n/* cli /// */\n//// cli";
        assert_eq!(
            without_docs(text).expect("literal and ordinary comments"),
            text
        );
    }
}
