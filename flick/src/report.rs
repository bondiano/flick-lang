//! Diagnostics on stderr and the exit code they imply.

use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::{env, io};

use flick_core::diagnostics::{Diagnostic, Severity, codes, render as renderers};
use flick_core::source::SourceMap;

use crate::args::{Color, DiagnosticsFormat, GlobalArguments};

pub fn render(global: &GlobalArguments, sources: &SourceMap, diagnostics: &[Diagnostic]) {
    let promoted;
    let diagnostics = if global.warnings_as_errors {
        promoted = promote(diagnostics);
        &promoted[..]
    } else {
        diagnostics
    };
    let out = match global.diagnostics {
        DiagnosticsFormat::Json => renderers::render_json(sources, diagnostics),
        DiagnosticsFormat::Human => {
            let color = match global.color {
                Color::Always => true,
                Color::Never => false,
                Color::Auto => io::stderr().is_terminal() && env::var_os("NO_COLOR").is_none(),
            };
            renderers::render_human(sources, diagnostics, color)
        }
    };
    // stderr closed: nothing useful left to do with the error
    let _ = io::stderr().lock().write_all(out.as_bytes());
}

/// Warnings as errors, for `--warnings-as-errors`.
pub fn promote(diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .cloned()
        .map(|mut d| {
            if d.severity == Severity::Warning {
                d.severity = Severity::Error;
            }
            d
        })
        .collect()
}

/// A line of service output or of a child process on stderr: as is in
/// human mode, an event of type `kind` (`log`, `application-stderr`,
/// `package-manager-output`) without ANSI codes in JSON mode.
pub fn event(global: &GlobalArguments, kind: &str, channel: &str, text: &str) {
    let out = match global.diagnostics {
        DiagnosticsFormat::Human => {
            if text.ends_with('\n') {
                text.to_owned()
            } else {
                format!("{text}\n")
            }
        }
        DiagnosticsFormat::Json => {
            let event = serde_json::json!({
                "schema-version": renderers::SCHEMA_VERSION,
                "type": kind,
                "channel": channel,
                "text": strip_ansi(text.strip_suffix('\n').unwrap_or(text)),
            });
            format!("{event}\n")
        }
    };
    let _ = io::stderr().lock().write_all(out.as_bytes());
}

/// A service message: dropped with `--quiet`.
pub fn log(global: &GlobalArguments, text: &str) {
    if !global.quiet {
        event(global, "log", "flick", text);
    }
}

/// `text` without ANSI escape sequences.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // parameters and intermediates end at a final byte in `@`..=`~`
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            chars.next();
        }
    }
    out
}

/// Errors that mean a wrong invocation, configuration or missing dependency
/// rather than a problem in the code: exit code `2`.
const USAGE: [codes::Code; 12] = [
    codes::P005,
    codes::P006,
    codes::P007,
    codes::P008,
    codes::P009,
    codes::P010,
    codes::P011,
    codes::P015,
    codes::P016,
    codes::P017,
    codes::P018,
    codes::P019,
];

/// `3` for an internal tooling error, `2` for a usage error, `1` for any
/// other error, else `0`.
pub fn exit_code(diagnostics: &[Diagnostic]) -> ExitCode {
    ExitCode::from(code(diagnostics))
}

/// [`exit_code`] as a number.
pub fn code(diagnostics: &[Diagnostic]) -> u8 {
    if diagnostics.iter().any(|d| d.code == codes::P003) {
        3
    } else if diagnostics.iter().any(|d| USAGE.contains(&d.code)) {
        2
    } else {
        u8::from(diagnostics.iter().any(|d| d.severity == Severity::Error))
    }
}
