//! Arguments of the project, run, dependency and tooling commands.

use clap::{Args, Subcommand, ValueEnum};

use super::ProfileArgument;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Template {
    App,
    /// An application whose `main` parses its command line with a `cli` block.
    Cli,
    /// A server and a browser page sharing the calls of a `server fn`
    /// module; `flick dev` runs both.
    Fullstack,
    Lib,
}

#[derive(Args)]
pub struct NewArguments {
    /// The directory to create.
    pub directory: String,

    /// An application with `main`, a command-line application, a server
    /// with a browser page, or a library.
    #[arg(long, value_enum, default_value_t = Template::App)]
    pub template: Template,
}

#[derive(Args)]
pub struct InitArguments {
    /// An application with `main`, a command-line application, a server
    /// with a browser page, or a library.
    #[arg(long, value_enum, default_value_t = Template::App)]
    pub template: Template,
}

#[derive(Args)]
pub struct RunArguments {
    /// The entry of the project to run, a name of `entries`; needed when
    /// it has several.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// The module whose `main` runs, instead of the project's entries.
    #[arg(long, value_name = "MODULE")]
    pub entry: Option<String>,

    /// `debug` or `release`; by default `build.profile` of the project.
    #[arg(long, value_enum)]
    pub profile: Option<ProfileArgument>,

    /// Sends the spans of the application, and the debug log of its atom
    /// writes, to the Effect `DevTools` listening at this URL.
    #[arg(long, value_name = "URL", num_args = 0..=1, default_missing_value = "ws://localhost:34437")]
    pub devtools: Option<String>,

    /// Arguments of the application, after `--`.
    #[arg(last = true)]
    pub args: Vec<String>,
}

#[derive(Args)]
pub struct CleanArguments {
    /// Also remove the project's Flick cache.
    #[arg(long)]
    pub cache: bool,
}

#[derive(Args)]
pub struct InstallArguments {
    /// Install exactly the lockfile: no lockfile, or one out of date with
    /// `package.json`, is an error.
    #[arg(long)]
    pub locked: bool,

    /// Install only from the local store.
    #[arg(long)]
    pub offline: bool,
}

#[derive(Args)]
pub struct AddArguments {
    /// npm package specs, such as `date-fns` or `date-fns@4`.
    #[arg(required = true)]
    pub packages: Vec<String>,

    /// Add to `devDependencies`.
    #[arg(long)]
    pub dev: bool,
}

#[derive(Args)]
pub struct PackagesArguments {
    /// Package names.
    pub packages: Vec<String>,
}

#[derive(Args)]
pub struct DepsArguments {
    /// Why this package is installed.
    #[arg(long, value_name = "PACKAGE")]
    pub why: Option<String>,
}

#[derive(Args)]
pub struct OutDirArguments {
    /// Directory to write into.
    #[arg(long, value_name = "DIR")]
    pub out_dir: String,
}

#[derive(Subcommand)]
pub enum SchemaCommands {
    /// Export the static wire form of a JSON codec.
    Export(SchemaExportArguments),
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SchemaTarget {
    #[value(name = "json-schema-2020-12")]
    JsonSchema,
    /// An `HttpApi.Api` as an `OpenAPI` 3.1 document.
    #[value(name = "openapi-3.1")]
    OpenApi,
}

#[derive(Args)]
pub struct SchemaExportArguments {
    /// `Module.value`: a `PureCodec Json a` for JSON Schema, an
    /// `HttpApi.Api gs` for `OpenAPI`.
    pub codec: String,

    #[arg(long, value_enum)]
    pub target: SchemaTarget,

    /// The file to write.
    #[arg(long, value_name = "FILE")]
    pub out: String,
}

#[derive(Args)]
pub struct DiffArguments {
    /// The git revision to compare with; the latest tag by default.
    #[arg(value_name = "REF")]
    pub git_ref: Option<String>,
}

#[derive(Args)]
pub struct ExplainArguments {
    /// A diagnostic code, such as `FLICK-T003`.
    pub code: String,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Representation {
    Ast,
    TypedIr,
    Js,
    Ts,
    Iface,
    Mapping,
}

#[derive(Args)]
pub struct InspectArguments {
    /// The module, such as `App.Main`.
    pub module: String,

    /// What to print.
    #[arg(long, value_enum)]
    pub emit: Representation,
}

#[derive(Args)]
pub struct LspArguments {
    /// Talk the protocol over stdin and stdout, the only transport.
    #[arg(long, required = true)]
    pub stdio: bool,
}
