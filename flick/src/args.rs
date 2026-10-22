//! Command-line arguments: options shared by all commands and one argument
//! struct per command.

use clap::{Args, Parser, Subcommand, ValueEnum};

/// The Flick compiler and toolchain.
#[derive(Parser)]
#[command(name = "flick", version = version())]
pub struct CommandArguments {
    #[command(flatten)]
    pub global: GlobalArguments,

    #[command(subcommand)]
    pub cmd: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Check Flick source files and report diagnostics.
    Check(CheckArguments),
    /// Format Flick source files in place.
    Fmt(FmtArguments),
}

#[derive(Args)]
pub struct GlobalArguments {
    /// Format of diagnostics on stderr.
    #[arg(long, global = true, value_enum, default_value_t = DiagnosticsFormat::Human)]
    pub diagnostics: DiagnosticsFormat,

    /// Colorize human-readable output.
    #[arg(long, global = true, value_enum, default_value_t = Color::Auto)]
    pub color: Color,

    /// Print less service output.
    #[arg(long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Print more service output.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Project directory or its `flick.toml`; by default the nearest
    /// `flick.toml` from the working directory upwards.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CheckArguments {
    /// Files or directories to check, with everything they import; by
    /// default the project's source and test roots.
    pub paths: Vec<String>,
}

#[derive(Args)]
pub struct FmtArguments {
    /// Files, or directories to search for `.fl` files.
    #[arg(required_unless_present = "stdin", conflicts_with = "stdin")]
    pub paths: Vec<String>,

    /// Only check: exit with 1 if a file is not formatted, write nothing.
    #[arg(long)]
    pub check: bool,

    /// Format source from stdin to stdout (for editors).
    #[arg(long)]
    pub stdin: bool,

    /// Path of the `--stdin` source, used in diagnostics. Requires `--stdin`.
    #[arg(long, value_name = "FILE")]
    pub stdin_filepath: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum DiagnosticsFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Color {
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
