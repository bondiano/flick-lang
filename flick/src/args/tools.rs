//! Arguments of the project, run, dependency and tooling commands.

use std::collections::BTreeMap;

use clap::{Args, ValueEnum};

use super::ProfileArgument;
use flick_core::project::config::Source;

#[derive(Args, Clone)]
pub struct ScaffoldArguments {
    /// Target backend whose templates to use.
    #[arg(long, value_name = "NAME", conflicts_with = "from")]
    pub target: Option<String>,

    /// Template name provided by the selected backend or `--from` package.
    #[arg(long, value_name = "NAME")]
    pub template: Option<String>,

    /// Third-party template package: local directory, tarball URL (with `--sha256`),
    /// or git repository URL (with `--tag`).
    #[arg(long, value_name = "SOURCE", conflicts_with = "target")]
    pub from: Option<String>,

    /// Git tag for a `--from` git repository.
    #[arg(long, requires = "from", conflicts_with = "sha256")]
    pub tag: Option<String>,

    /// Subdirectory inside a `--from` git repository.
    #[arg(long, requires = "tag")]
    pub subdir: Option<String>,

    /// SHA-256 hex digest of a `--from` tarball URL.
    #[arg(long, requires = "from", conflicts_with = "tag")]
    pub sha256: Option<String>,
}

#[derive(Args)]
pub struct NewArguments {
    /// The directory to create.
    pub directory: String,

    #[command(flatten)]
    pub scaffold: ScaffoldArguments,
}

#[derive(Args)]
pub struct InitArguments {
    #[command(flatten)]
    pub scaffold: ScaffoldArguments,
}

#[derive(Args)]
pub struct RunArguments {
    /// The entry of the project to run, a name of `entries`, needed when
    /// it has several; else a script of `[scripts]` (only for `run`).
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// The module whose `main` runs, instead of the project's entries.
    #[arg(long, value_name = "MODULE")]
    pub entry: Option<String>,

    /// Target backend; by default `build.target` of the project.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,

    /// VM execution tier for `--target vm` (`0` or `tier0`; defaults to tier 0).
    #[arg(long = "vm-tier", value_name = "TIER")]
    pub vm_tier: Option<String>,

    /// `debug` or `release`; by default `build.profile` of the project.
    #[arg(long, value_enum)]
    pub profile: Option<ProfileArgument>,

    /// Ignore `@native` overrides and lower Flick fallback bodies (for conformance testing).
    #[arg(long)]
    pub no_native_overrides: bool,

    /// Backend-declared options parsed from the command line.
    #[arg(skip)]
    pub backend_options: BTreeMap<String, Vec<String>>,

    /// Arguments of the application, or the script's `"$@"`, after `--`.
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
    /// Install exactly the lockfile: a missing or out-of-date lockfile is an error.
    #[arg(long)]
    pub locked: bool,
}

#[derive(Args)]
pub struct AddArguments {
    /// Package specs for the selected backend, or with a source option, the
    /// name of one Flick library.
    #[arg(required = true)]
    pub packages: Vec<String>,

    /// Add as a development dependency when the backend supports it.
    #[arg(long, conflicts_with_all = ["git", "url", "path"])]
    pub dev: bool,

    /// A Flick library from a tag of this git repository.
    #[arg(long, value_name = "URL", requires = "tag", conflicts_with_all = ["url", "path"])]
    pub git: Option<String>,

    /// The tag of `--git`.
    #[arg(long, requires = "git")]
    pub tag: Option<String>,

    /// The directory of the library in the repository of `--git`.
    #[arg(long, requires = "git")]
    pub subdir: Option<String>,

    /// A Flick library from a gzipped tarball at this URL.
    #[arg(long, value_name = "URL", requires = "sha256", conflicts_with = "path")]
    pub url: Option<String>,

    /// The sha256 of the tarball of `--url`.
    #[arg(long, requires = "url")]
    pub sha256: Option<String>,

    /// A Flick library in this directory.
    #[arg(long, value_name = "DIR")]
    pub path: Option<String>,
}

impl AddArguments {
    /// The source of the Flick library to add, when an option names one.
    pub fn source(&self) -> Option<Source> {
        if let Some(path) = &self.path {
            return Some(Source::Path(path.clone()));
        }
        if let (Some(url), Some(tag)) = (&self.git, &self.tag) {
            return Some(Source::Git {
                url: url.clone(),
                tag: tag.clone(),
                subdir: self.subdir.clone(),
            });
        }
        let (url, sha256) = (self.url.clone()?, self.sha256.clone()?);
        Some(Source::Url { url, sha256 })
    }
}

#[derive(Args)]
pub struct RemoveArguments {
    /// Package names.
    #[arg(required = true)]
    pub packages: Vec<String>,
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

#[derive(Args)]
pub struct PackArguments {
    #[command(flatten)]
    pub output: OutDirArguments,
    /// Target backend; by default `build.target` of the project.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,
}

/// Documentation rendered from checked public interfaces.
#[derive(Args)]
pub struct DocArguments {
    #[command(flatten)]
    pub output: OutDirArguments,
    /// URL of this project's root at a source revision (for example a GitHub blob URL).
    #[arg(long, value_name = "URL")]
    pub source_base_url: Option<String>,
    /// Check fenced Flick examples in public documentation in this package context.
    #[arg(long)]
    pub check_examples: bool,
    /// Target backend for `--check-examples`; by default `build.target` of the
    /// project.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,
    /// Document the loaded standard library rather than a user project.
    #[arg(long)]
    pub std: bool,
}

#[derive(Args)]
pub struct DiffArguments {
    /// The git revision to compare with; the latest tag by default.
    #[arg(value_name = "REF")]
    pub git_ref: Option<String>,
}

#[derive(Args)]
pub struct ExplainArguments {
    /// A diagnostic code, such as `FLICK-T003`, or one of a plugin, such as
    /// `std/L001`.
    pub code: String,
}

#[derive(Args)]
pub struct ExpandArguments {
    /// Files or directories whose plugin blocks to show; by default the
    /// project's source and test roots.
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Representation {
    Ast,
    TypedIr,
    Output,
    Iface,
    Variance,
    Mapping,
}

#[derive(Args)]
pub struct InspectArguments {
    /// The module, such as `App.Main`.
    pub module: String,

    /// What to print.
    #[arg(long, value_enum)]
    pub emit: Representation,

    /// Target backend for `--emit output`; by default `build.target` of the
    /// project.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,

    /// Ignore `@native` overrides and lower Flick fallback bodies (for conformance testing).
    #[arg(long)]
    pub no_native_overrides: bool,

    /// Backend-declared options parsed from the command line.
    #[arg(skip)]
    pub backend_options: BTreeMap<String, Vec<String>>,
}

#[derive(Args)]
pub struct DoctorArguments {
    /// Target backend to inspect; by default the project's configured target.
    #[arg(long, value_name = "NAME")]
    pub target: Option<String>,
}

#[derive(Args)]
pub struct LspArguments {
    /// Talk the protocol over stdin and stdout, the only transport.
    #[arg(long, required = true)]
    pub stdio: bool,
}
