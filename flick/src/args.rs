//! Command-line arguments: options shared by all commands and one argument
//! struct per command.

use std::collections::BTreeMap;
use std::ffi::OsString;

use clap::{Args, Parser, Subcommand, ValueEnum};

mod tools;

pub use tools::*;

/// The Flick compiler and toolchain.
#[derive(Parser)]
#[command(name = "flick", version)]
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
    /// Check the proofs of the project's proof root, with the code they name.
    Prove(ProveArguments),
    /// Compile the project, or the given files, with the selected target backend.
    Build(BuildArguments),
    /// Format Flick source files in place.
    Fmt(FmtArguments),
    /// Run the tests of the project's test root.
    Test(TestArguments),
    /// Create a project in a new directory.
    New(NewArguments),
    /// Add a project to the working directory; existing files are kept.
    Init(InitArguments),
    /// Build the app with the selected target backend and run its `main`,
    /// or run a script of `[scripts]` that is not an entry's name.
    Run(RunArguments),
    /// Rebuild on changes and replace the running app after its shutdown.
    Dev(RunArguments),
    /// Remove the build artifacts of the project.
    Clean(CleanArguments),
    /// Install the project's dependencies; at a workspace root, of every member.
    Install(InstallArguments),
    /// Add dependencies (Flick libraries with a source option, or target packages).
    Add(AddArguments),
    /// Remove direct dependencies.
    Remove(RemoveArguments),
    /// Update dependencies within their declared ranges.
    Update(PackagesArguments),
    /// Show the installed dependencies and their Flick libraries.
    Deps(DepsArguments),
    /// Pack a deterministic Flick source tarball (and any artifact from the selected backend).
    Pack(PackArguments),
    /// Show the plugins of the project's path, git and url dependencies, where
    /// each comes from, and whether `flick.lock` pins the build of its
    /// component.
    Plugins,
    /// Compare the public API with a git revision and name the version bump.
    Diff(DiffArguments),
    /// Write documentation of the public API.
    Doc(DocArguments),
    /// Explain a diagnostic code.
    Explain(ExplainArguments),
    /// Print the code each plugin block of the files stands for, as its
    /// plugin gives it.
    Expand(ExpandArguments),
    /// Check the compiler, plugins, and the selected backend's toolchain.
    Doctor(DoctorArguments),
    /// Print a representation of a module: AST, typed IR, backend output,
    /// interface, variance or source mapping.
    Inspect(InspectArguments),
    /// Run the language server.
    Lsp(LspArguments),
    /// A command the project's backend or one of its plugins adds, flat or
    /// as `contributor:command`.
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

#[derive(Args, Clone)]
#[allow(clippy::struct_excessive_bools, reason = "one per command-line flag")]
pub struct GlobalArguments {
    /// Format of diagnostics on stderr.
    #[arg(long, global = true, value_enum, default_value_t = DiagnosticsFormat::Human)]
    pub diagnostics: DiagnosticsFormat,

    /// Colorize human-readable output.
    #[arg(long, global = true, value_enum, default_value_t = Color::Auto)]
    pub color: Color,

    /// Suppress service output; diagnostics are still reported.
    #[arg(
        long,
        visible_alias = "silent",
        global = true,
        conflicts_with = "verbose"
    )]
    pub quiet: bool,

    /// Print more service output.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Report on stderr how long each phase and the slowest modules took,
    /// and which input made the build cache miss.
    #[arg(long, global = true)]
    pub timings: bool,

    /// Project directory or its `flick.toml`; by default the nearest
    /// `flick.toml` from the working directory upwards.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<String>,

    /// The workspace member to run on, the name of its directory; at the
    /// root of a workspace `check`, `build`, `test`, `fmt`, `clean` and
    /// `install` otherwise run on every member.
    // its own id: a positional `package` of a command would otherwise
    // fill this global too
    #[arg(
        id = "member",
        long = "package",
        short = 'p',
        global = true,
        value_name = "NAME",
        conflicts_with = "project"
    )]
    pub package: Option<String>,

    /// Treat warnings as errors.
    #[arg(long, global = true)]
    pub warnings_as_errors: bool,

    /// Reach no network: plugins answer from the project's snapshots, and
    /// `install` installs only from the local store. `CI` set means this
    /// for plugins too.
    #[arg(long, global = true)]
    pub offline: bool,

    /// Let the selected backend run this host tool in a build
    /// outside a project; a project allows its tools in `[tools]`.
    #[arg(long = "allow-tool", global = true, value_name = "TOOL")]
    pub allow_tool: Vec<String>,
}

#[derive(Args)]
pub struct CheckArguments {
    /// Files or directories to check, with everything they import; by
    /// default the project's source and test roots.
    pub paths: Vec<String>,

    /// Check again whenever a source file changes.
    #[arg(long)]
    pub watch: bool,

    /// Apply the fixes that keep the program's meaning, such as removing a
    /// type annotation the value already has, then report what is left.
    #[arg(long, conflicts_with = "watch")]
    pub fix: bool,

    /// With `--fix`, apply only the fixes of diagnostics with these codes,
    /// such as `E002,F001`: a migration that changes nothing else.
    #[arg(long, requires = "fix", value_delimiter = ',', value_name = "CODES")]
    pub only: Vec<String>,
}

#[derive(Args)]
pub struct ProveArguments {
    /// Proof modules or directories to check, with everything they import;
    /// by default the project's proof root (`project.proof-dir`).
    pub paths: Vec<String>,

    /// Check again whenever a source file changes.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Args)]
pub struct BuildArguments {
    /// Files or directories to compile, with everything they import; by
    /// default the project's source root.
    pub paths: Vec<String>,

    /// Target backend; by default `build.target` of the project.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,

    /// Directory of the output; by default `build.out-dir` of the project.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<String>,

    /// Also write the IR the backend gets, `program.ir.json` or
    /// `program.ir.cbor`; optimized in a release build.
    #[arg(long, value_enum, value_name = "FORMAT", num_args = 0..=1, default_missing_value = "json")]
    pub emit_ir: Option<IrFormat>,

    /// `debug`, the reference lowering, or `release`, optimized; by default
    /// `build.profile` of the project.
    #[arg(long, value_enum)]
    pub profile: Option<ProfileArgument>,

    /// Which top-level values a release build labels `Module.name`; by
    /// default `build.labels` of the project.
    #[arg(long, value_enum)]
    pub labels: Option<LabelsArgument>,

    /// Print the compiler's input inventory as JSON without building.
    #[arg(long, conflicts_with = "watch")]
    pub inputs: bool,

    /// Build again whenever a source file changes; a failed build keeps the
    /// last output.
    #[arg(long)]
    pub watch: bool,

    /// Ignore `@native` overrides and lower Flick fallback bodies (for conformance testing).
    #[arg(long)]
    pub no_native_overrides: bool,

    /// Backend-declared options parsed from the command line.
    #[arg(skip)]
    pub backend_options: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProfileArgument {
    Debug,
    Release,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LabelsArgument {
    All,
    Components,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum IrFormat {
    Json,
    Cbor,
}

#[derive(Args)]
pub struct FmtArguments {
    /// Files, or directories to search for `.fl` files; by default the
    /// project's source and test roots.
    #[arg(conflicts_with = "stdin")]
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
#[allow(clippy::struct_excessive_bools, reason = "one per command-line flag")]
pub struct TestArguments {
    /// Test files to run; by default every module of the test root.
    pub paths: Vec<String>,

    /// Run only the tests whose full name, `Module > suite > test`,
    /// contains this text.
    #[arg(long, value_name = "TEXT")]
    pub filter: Option<String>,

    /// Print the full name of each test the run would run, and run none.
    #[arg(long, conflicts_with = "watch")]
    pub list: bool,

    /// Fail, before running any test, when a test of the run is marked
    /// `only`: for CI, where a forgotten `test.only` would skip the rest.
    #[arg(long)]
    pub forbid_only: bool,

    /// How many tests run at a time.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub jobs: u32,

    /// Seed of the generators of property tests; printed when a test fails.
    #[arg(long)]
    pub seed: Option<u32>,

    /// The time each test without a `timeout` of its own has for its
    /// `before_each` hooks and its body: `500ms`, `5s`, `2m`, or a number of
    /// milliseconds. By default a test has no time limit.
    #[arg(long, value_name = "DURATION", value_parser = millis)]
    pub timeout: Option<f64>,

    /// How results are written.
    #[arg(long, value_enum, default_value_t = Reporter::Human)]
    pub reporter: Reporter,

    /// Run again whenever a source file changes, once the previous run has
    /// closed its scopes.
    #[arg(long)]
    pub watch: bool,

    /// `debug` or `release`; by default `build.profile` of the project.
    #[arg(long, value_enum)]
    pub profile: Option<ProfileArgument>,

    /// Target backend; by default `build.target` of the project.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,

    /// Ignore `@native` overrides and lower Flick fallback bodies (for conformance testing).
    #[arg(long)]
    pub no_native_overrides: bool,

    /// Backend-declared options parsed from the command line.
    #[arg(skip)]
    pub backend_options: BTreeMap<String, Vec<String>>,
}

/// The milliseconds of a duration written `500ms`, `5s`, `2m` or `500`.
fn millis(text: &str) -> Result<f64, String> {
    let (number, unit) = [("ms", 1.0), ("s", 1000.0), ("m", 60_000.0)]
        .into_iter()
        .find_map(|(suffix, unit)| Some((text.strip_suffix(suffix)?, unit)))
        .unwrap_or((text, 1.0));
    match number.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => Ok(n * unit),
        _ => Err(format!(
            "`{text}` is not a duration: write `500ms`, `5s` or `2m`"
        )),
    }
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
