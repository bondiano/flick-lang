//! Bootstrap front-end and codegen for a tiny entry-module subset, so the e2e
//! pipeline `.fl -> ESM -> Node` exists before the real compiler.
//!
//! shortcut: bootstrap subset only, replaced by the M1 parser and M7 codegen.
//!
//! Accepted shape (blank lines and `//` comment lines are skipped anywhere):
//!
//! ```text
//! module App.Main exposing (main)
//!
//! main : Unit ->{none; AppError} Unit
//! let main () =
//!   Platform.stdout_line "…"
//!   Platform.stderr_line "…"
//!   App.fail "…"
//! ```

use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, SourceMap, Span};

const HEADER: [&str; 3] = [
    "module App.Main exposing (main)",
    "main : Unit ->{none; AppError} Unit",
    "let main () =",
];

const CALLS: [(&str, &str); 3] = [
    ("Platform.stdout_line", "rt.stdoutLine"),
    ("Platform.stderr_line", "rt.stderrLine"),
    ("App.fail", "rt.appFail"),
];

/// Compiles an entry module to ESM that runs `main` through the runtime's entry runner.
pub fn compile(sources: &SourceMap, file: FileId) -> Result<String, Vec<Diagnostic>> {
    let text = &sources.get(file).text;
    let mut lines = text
        .split_inclusive('\n')
        .scan(0, |start, raw| {
            let line_start = *start;
            *start += raw.len();
            let line = raw.trim_end_matches(['\n', '\r']);
            let indent = line.len() - line.trim_start().len();
            Some((line_start + indent, line.trim()))
        })
        .filter(|(_, l)| !l.is_empty() && !l.starts_with("//"));

    let mut errors = Vec::new();
    for expected in HEADER {
        match lines.next() {
            Some((_, line)) if line == expected => {}
            Some((at, line)) => errors.push(unsupported(
                file,
                at,
                line.len(),
                format!("expected `{expected}`"),
            )),
            None => errors.push(unsupported(
                file,
                text.len(),
                0,
                format!("expected `{expected}` before end of file"),
            )),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut calls = Vec::new();
    for (at, line) in lines {
        match statement(line) {
            Ok(call) => calls.push(call),
            Err((offset, len, message)) => {
                errors.push(unsupported(file, at + offset, len, message));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(format!(
        "import * as rt from \"@flick-lang/runtime\";\n\n\
         export const main = () => rt.seq([{}]);\n\n\
         await rt.runMain(main);\n",
        calls.join(", ")
    ))
}

type StatementError = (usize, usize, String);

/// One body line: `<Callee> "<string>"`, returned as a JS call expression.
fn statement(line: &str) -> Result<String, StatementError> {
    let (callee, arg) = line.split_once(' ').ok_or_else(|| {
        (
            0,
            line.len(),
            "expected a call with a string argument".to_owned(),
        )
    })?;
    let js = CALLS
        .iter()
        .find(|(name, _)| *name == callee)
        .map(|(_, js)| js)
        .ok_or_else(|| {
            (
                0,
                callee.len(),
                format!("`{callee}` is not supported by the bootstrap front-end"),
            )
        })?;
    let arg_at = callee.len() + 1;
    let value = string_literal(arg).map_err(|(o, l, m)| (arg_at + o, l, m))?;
    let value = serde_json::to_string(&value).expect("string serializes");
    Ok(format!("{js}({value})"))
}

/// A whole `"…"` literal with the escapes `\n \t \r \\ \" \0 \u{…}`.
fn string_literal(src: &str) -> Result<String, StatementError> {
    let bad = |at: usize, len: usize, msg: &str| (at, len, msg.to_owned());
    let body = src
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .filter(|_| src.len() >= 2)
        .ok_or_else(|| bad(0, src.len(), "expected a single string literal"))?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        let at = i + 1;
        match c {
            '"' => return Err(bad(at, 1, "unescaped `\"` inside string literal")),
            '\\' => {
                let decoded = match chars.next().map(|(_, e)| e) {
                    Some('n') => '\n',
                    Some('t') => '\t',
                    Some('r') => '\r',
                    Some('\\') => '\\',
                    Some('"') => '"',
                    Some('0') => '\0',
                    Some('u') => {
                        let rest = &body[i + 2..];
                        let hex = rest
                            .strip_prefix('{')
                            .and_then(|r| r.split_once('}'))
                            .map(|(hex, _)| hex)
                            .ok_or_else(|| bad(at, 2, "expected `\\u{…}`"))?;
                        let len = hex.len() + 4;
                        let c = u32::from_str_radix(hex, 16)
                            .ok()
                            .and_then(char::from_u32)
                            .ok_or_else(|| bad(at, len, "invalid unicode scalar in `\\u{…}`"))?;
                        chars.nth(hex.len() + 1);
                        c
                    }
                    _ => return Err(bad(at, 2, "unknown escape sequence")),
                };
                out.push(decoded);
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

fn unsupported(file: FileId, at: usize, len: usize, message: String) -> Diagnostic {
    let span = |n: usize| u32::try_from(n).expect("source offsets fit in u32");
    Diagnostic::error(
        codes::S002,
        message,
        Span::new(file, span(at), span(at + len)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_str(text: &str) -> Result<String, Vec<Diagnostic>> {
        let mut sources = SourceMap::default();
        let file = sources.add("t.fl", text);
        compile(&sources, file)
    }

    const HEAD: &str = "module App.Main exposing (main)\n\n// c\nmain : Unit ->{none; AppError} Unit\nlet main () =\n";

    #[test]
    fn compiles_calls_with_escapes() {
        let js = compile_str(&format!(
            "{HEAD}  Platform.stdout_line \"a\\n\\u{{1F600}}\\\"\"\r\n  App.fail \"b\"\n"
        ))
        .expect("compiles");
        assert!(
            js.contains(r#"rt.seq([rt.stdoutLine("a\n😀\""), rt.appFail("b")])"#),
            "{js}"
        );
    }

    #[test]
    fn reports_spans_of_unsupported_parts() {
        let src = format!("{HEAD}  Log.info \"x\"\n  App.fail \"\\q\"\n");
        let errors = compile_str(&src).expect_err("rejected");
        let texts: Vec<_> = errors
            .iter()
            .map(|d| &src[d.span.expect("has span").range()])
            .collect();
        assert_eq!(texts, ["Log.info", "\\q"]);
    }

    #[test]
    fn reports_wrong_header() {
        let errors = compile_str("module Foo\n").expect_err("rejected");
        assert_eq!(errors.len(), 3);
        assert!(errors[2].message.contains("before end of file"));
    }
}
