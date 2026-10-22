use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use flick_core::diagnostics::{Diagnostic, Severity, codes, render};
use flick_core::source::SourceMap;

/// The Flick compiler and toolchain.
#[derive(Parser)]
#[command(name = "flick", version = version())]
struct Cli {
    /// Format of diagnostics on stderr.
    #[arg(long, global = true, value_enum, default_value_t = DiagnosticsFormat::Human)]
    diagnostics: DiagnosticsFormat,

    /// Colorize human-readable output.
    #[arg(long, global = true, value_enum, default_value_t = Color::Auto)]
    color: Color,

    /// Print less service output.
    #[arg(long, global = true, conflicts_with = "verbose")]
    quiet: bool,

    /// Print more service output.
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check Flick source files and report diagnostics.
    Check {
        /// Source files to check.
        #[arg(required = true)]
        paths: Vec<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum DiagnosticsFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum Color {
    Auto,
    Always,
    Never,
}

fn version() -> String {
    format!(
        "{} (Flick {})",
        env!("CARGO_PKG_VERSION"),
        flick_core::LANGUAGE_VERSION
    )
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match &cli.command {
        Command::Check { paths } => check(&cli, paths),
    }
}

fn check(cli: &Cli, paths: &[String]) -> ExitCode {
    let mut sources = SourceMap::default();
    let mut diagnostics = Vec::new();
    for path in paths {
        match std::fs::read(path) {
            Ok(bytes) => match sources.add_bytes(path.as_str(), bytes) {
                (_, Some(utf8)) => diagnostics.push(utf8),
                (file, None) => diagnostics.extend(flick_core::check(&sources, file)),
            },
            Err(err) => diagnostics.push(Diagnostic::new(
                codes::P001,
                Severity::Error,
                format!("cannot read `{path}`: {err}"),
            )),
        }
    }
    report(cli, &sources, &diagnostics);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn report(cli: &Cli, sources: &SourceMap, diagnostics: &[Diagnostic]) {
    let out = match cli.diagnostics {
        DiagnosticsFormat::Json => render::render_json(sources, diagnostics),
        DiagnosticsFormat::Human => {
            let color = match cli.color {
                Color::Always => true,
                Color::Never => false,
                Color::Auto => {
                    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
                }
            };
            render::render_human(sources, diagnostics, color)
        }
    };
    // stderr closed: nothing useful left to do with the error
    let _ = std::io::stderr().lock().write_all(out.as_bytes());
}
