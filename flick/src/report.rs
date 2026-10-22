//! Diagnostics on stderr and the exit code they imply.

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use flick_core::diagnostics::{Diagnostic, Severity, codes, render as renderers};
use flick_core::source::SourceMap;

use crate::args::{Color, DiagnosticsFormat, GlobalArguments};

pub fn render(global: &GlobalArguments, sources: &SourceMap, diagnostics: &[Diagnostic]) {
    let out = match global.diagnostics {
        DiagnosticsFormat::Json => renderers::render_json(sources, diagnostics),
        DiagnosticsFormat::Human => {
            let color = match global.color {
                Color::Always => true,
                Color::Never => false,
                Color::Auto => {
                    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
                }
            };
            renderers::render_human(sources, diagnostics, color)
        }
    };
    // stderr closed: nothing useful left to do with the error
    let _ = std::io::stderr().lock().write_all(out.as_bytes());
}

/// Errors that mean a wrong invocation, configuration or missing dependency
/// rather than a problem in the code: exit code `2` (§12).
const USAGE: [codes::Code; 7] = [
    codes::P005,
    codes::P006,
    codes::P007,
    codes::P008,
    codes::P009,
    codes::P010,
    codes::P011,
];

/// `3` for an internal tooling error, `2` for a usage error, `1` for any
/// other error, else `0`.
pub fn exit_code(diagnostics: &[Diagnostic]) -> ExitCode {
    if diagnostics.iter().any(|d| d.code == codes::P003) {
        ExitCode::from(3)
    } else if diagnostics.iter().any(|d| USAGE.contains(&d.code)) {
        ExitCode::from(2)
    } else if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
