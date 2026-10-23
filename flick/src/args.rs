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
    /// Compile the project, or the given files, to ES modules.
    Build(BuildArguments),
    /// Format Flick source files in place.
    Fmt(FmtArguments),
    /// Run the tests of the project's test root.
    Test(TestArguments),
    /// Bindings to npm packages from their `.d.ts`.
    #[command(subcommand)]
    Ffi(FfiCommands),
}

#[derive(Subcommand)]
pub enum FfiCommands {
    /// Read the `.d.ts` of an installed package and write
    /// `<out>/<Pkg>/Generated.fl` with a coverage report.
    Import(FfiImportArguments),
    /// Check generated bindings, reviewed bindings and their policies
    /// against the installed packages.
    Check(FfiCheckArguments),
}

#[derive(Args)]
pub struct FfiImportArguments {
    /// The npm package, installed in the project.
    pub package: String,

    /// Directory that gets `<Pkg>/Generated.fl`.
    #[arg(long, value_name = "DIR")]
    pub out: String,

    /// Import only these exports.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    pub select: Option<Vec<String>>,
}

#[derive(Args)]
pub struct FfiCheckArguments {
    /// Files or directories to check; by default the project's source root.
    pub paths: Vec<String>,
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
pub struct BuildArguments {
    /// Files or directories to compile, with everything they import; by
    /// default the project's source root.
    pub paths: Vec<String>,

    /// Directory of the output; by default `build.out-dir` of the project.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<String>,

    /// Emit JavaScript or TypeScript; by default `build.emit` of the project.
    #[arg(long, value_enum)]
    pub emit: Option<EmitArgument>,

    /// Also write `.d.ts` files, the public TypeScript interface; an export
    /// without a TypeScript form is an error.
    #[arg(long)]
    pub emit_dts: bool,

    /// Also write `A/B.promise.js`: Promise façades of runtime functions.
    #[arg(long)]
    pub promise_facade: bool,

    /// Compile pure codecs ahead of use in the app's runner.
    #[arg(long)]
    pub schema_aot: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EmitArgument {
    Js,
    Ts,
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

#[derive(Args)]
pub struct TestArguments {
    /// Test files to run; by default every module of the test root.
    pub paths: Vec<String>,

    /// Run only the tests whose full name contains this text.
    #[arg(long, value_name = "TEXT")]
    pub filter: Option<String>,

    /// How many tests run at a time.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub jobs: u32,

    /// Seed of the generators of property tests; printed when a test fails.
    #[arg(long)]
    pub seed: Option<u32>,

    /// How results are written.
    #[arg(long, value_enum, default_value_t = Reporter::Human)]
    pub reporter: Reporter,

    /// Run again whenever a source file changes, once the previous run has
    /// closed its scopes.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Reporter {
    Human,
    Json,
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
