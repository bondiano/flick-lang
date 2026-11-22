//! Host tools that build steps and launches run. A tool is found on `PATH`
//! and pinned by the digest and version of its executable; a step runs it
//! with a cleared environment that keeps only what its adapter names.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

use flick_backend::{Launch, OptionValue, Toolchain, ToolchainStep};
use sha2::{Digest, Sha256};

/// What the host knows of one tool.
struct Adapter {
    id: &'static str,
    /// The arguments that print its version.
    version: &'static [&'static str],
    parse: fn(&str) -> Option<String>,
    /// Variables a step inherits from the environment of `flick`.
    inherit: &'static [&'static str],
    /// Variables a step may set.
    settable: &'static [&'static str],
    /// Variables a launch may set on this runtime.
    launch_settable: &'static [&'static str],
    /// Runtime flags a launch may pass before its artifact.
    validate_launch_args: fn(&[String]) -> Result<(), String>,
    /// Variables the host pins for this tool on every run, version query included.
    fixed: &'static [(&'static str, &'static str)],
}

fn no_launch_args(args: &[String]) -> Result<(), String> {
    match args.first() {
        Some(arg) => Err(format!("launch refuses runtime argument `{arg}`")),
        None => Ok(()),
    }
}

fn valid_node_import_specifier(spec: &str) -> bool {
    if spec.is_empty()
        || spec.starts_with(['/', '\\', '.', '-'])
        || spec.contains([':', '\\', '\0', '?', '#', '%'])
        || spec.contains("..")
        || spec.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let mut segments = spec.split('/');
    let Some(first) = segments.next() else {
        return false;
    };
    if let Some(scope) = first.strip_prefix('@') {
        if scope.is_empty() || scope.starts_with(['.', '-', '@']) {
            return false;
        }
        let Some(pkg) = segments.next() else {
            return false;
        };
        if pkg.is_empty() || pkg.starts_with(['.', '-']) {
            return false;
        }
    } else if first.is_empty() {
        return false;
    }
    segments.all(|seg| !seg.is_empty() && seg != ".")
}

fn validate_node_launch_args(args: &[String]) -> Result<(), String> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--enable-source-maps" => i += 1,
            "--import" => {
                let Some(spec) = args.get(i + 1) else {
                    return Err("`node` launch argument `--import` requires a specifier".into());
                };
                if !valid_node_import_specifier(spec) {
                    return Err(format!(
                        "`node` launch refuses `--import` specifier `{spec}`"
                    ));
                }
                i += 2;
            }
            other => {
                return Err(format!("`node` launch refuses runtime argument `{other}`"));
            }
        }
    }
    Ok(())
}

/// Variables every tool needs to find its home, caches and network proxy.
const COMMON: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "SYSTEMROOT",
    "LOCALAPPDATA",
    "APPDATA",
    "XDG_CACHE_HOME",
    "TMPDIR",
    "TEMP",
    "TMP",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

const ADAPTERS: &[Adapter] = &[
    Adapter {
        id: "node",
        version: &["--version"],
        parse: |out| Some(out.trim().strip_prefix('v')?.to_owned()),
        inherit: &[],
        settable: &[],
        launch_settable: &["FLICK_DEVTOOLS"],
        validate_launch_args: validate_node_launch_args,
        fixed: &[],
    },
    Adapter {
        id: "go",
        // `go --version` is no flag of `go`
        version: &["version"],
        parse: |out| {
            let version = out.split_whitespace().nth(2)?.strip_prefix("go")?;
            // Go before 1.21 names a first release `go1.20`
            Some(if version.matches('.').count() == 1 {
                format!("{version}.0")
            } else {
                version.to_owned()
            })
        },
        inherit: &[
            "GOPATH",
            "GOCACHE",
            "GOMODCACHE",
            "GOPROXY",
            "GONOSUMDB",
            "GONOPROXY",
            "GOPRIVATE",
            "GOINSECURE",
        ],
        settable: &["GOFLAGS", "GOTOOLCHAIN", "CGO_ENABLED"],
        launch_settable: &[],
        validate_launch_args: no_launch_args,
        fixed: &[],
    },
    Adapter {
        id: "pnpm",
        version: &["--version"],
        parse: |out| {
            let version = out.trim();
            (!version.is_empty()).then(|| version.to_owned())
        },
        inherit: &[
            "PNPM_HOME",
            "NPM_CONFIG_REGISTRY",
            "NPM_CONFIG_STORE_DIR",
            "npm_config_registry",
            "npm_config_store_dir",
            "COREPACK_HOME",
            "NO_COLOR",
            "FORCE_COLOR",
        ],
        settable: &[],
        launch_settable: &[],
        validate_launch_args: no_launch_args,
        // Flick owns the version gate; even `pnpm --version` must not bootstrap or pin pnpm.
        fixed: &[
            ("npm_config_manage_package_manager_versions", "false"),
            ("PNPM_CONFIG_PM_ON_FAIL", "ignore"),
            ("COREPACK_ENABLE_PROJECT_SPEC", "0"),
            ("COREPACK_ENABLE_AUTO_PIN", "0"),
            ("COREPACK_ENABLE_NETWORK", "0"),
            ("COREPACK_DEFAULT_TO_LATEST", "0"),
        ],
    },
];

pub(crate) struct Tool {
    pub pin: Toolchain,
    path: PathBuf,
    adapter: &'static Adapter,
}

impl Tool {
    /// The tool `id` first on `PATH`; with `expected`, only that exact pin.
    pub(crate) fn find(id: &str, expected: Option<&Toolchain>) -> Result<Self, String> {
        let paths = env::var_os("PATH").unwrap_or_default();
        Self::on_path(id, &paths, expected)
    }

    fn on_path(id: &str, paths: &OsStr, expected: Option<&Toolchain>) -> Result<Self, String> {
        let adapter = (ADAPTERS.iter())
            .find(|a| a.id == id)
            .ok_or_else(|| format!("the host has no adapter for the tool `{id}`"))?;
        let names: Vec<String> = if cfg!(windows) {
            vec![format!("{id}.exe"), format!("{id}.cmd")]
        } else {
            vec![id.to_owned()]
        };
        let path = env::split_paths(paths)
            .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
            .find(|path| executable(path))
            .ok_or_else(|| format!("cannot find `{id}` on PATH"))?;
        Self::at(&path, adapter, expected)
    }

    /// A candidate with another digest is refused before it ever runs.
    fn at(
        path: &Path,
        adapter: &'static Adapter,
        expected: Option<&Toolchain>,
    ) -> Result<Self, String> {
        let differs = || {
            format!(
                "`{}` differs from its pinned version and digest",
                adapter.id
            )
        };
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        io::copy(
            &mut fs::File::open(&path).map_err(|e| e.to_string())?,
            &mut hash,
        )
        .map_err(|e| e.to_string())?;
        let digest = format!("{:x}", hash.finalize());
        if expected.is_some_and(|pin| pin.id != adapter.id || pin.sha256 != digest) {
            return Err(differs());
        }
        let mut query = Command::new(&path);
        query.args(adapter.version);
        for &(name, value) in adapter.fixed {
            query.env(name, value);
        }
        let output = query.output().map_err(|e| e.to_string())?;
        let version = (output.status.success())
            .then(|| String::from_utf8(output.stdout).ok())
            .flatten()
            .and_then(|out| (adapter.parse)(&out))
            .ok_or_else(|| format!("cannot query the version of `{}`", adapter.id))?;
        let tool = Self {
            pin: Toolchain {
                id: adapter.id.into(),
                version,
                sha256: digest,
            },
            path,
            adapter,
        };
        if expected.is_some_and(|expected| expected != &tool.pin) {
            return Err(differs());
        }
        Ok(tool)
    }

    /// The same tool, if its executable is unchanged since it was found.
    pub(crate) fn recheck(&self) -> Result<Self, String> {
        Self::at(&self.path, self.adapter, Some(&self.pin))
    }

    #[cfg(test)]
    pub(crate) fn at_path(id: &str, path: &Path) -> Result<Self, String> {
        let adapter = ADAPTERS.iter().find(|a| a.id == id).expect("an adapter");
        Self::at(path, adapter, None)
    }

    pub(crate) fn command(&self) -> Command {
        Command::new(&self.path)
    }

    /// The tool as a build step runs it: in `dir`, with only the variables
    /// of its adapter.
    pub(crate) fn step(&self, dir: &Path) -> Command {
        let mut command = self.command();
        command.current_dir(dir).env_clear();
        for name in COMMON.iter().chain(self.adapter.inherit) {
            if let Some(value) = env::var_os(name) {
                command.env(name, value);
            }
        }
        for &(name, value) in self.adapter.fixed {
            command.env(name, value);
        }
        command
    }

    /// The tool as a client entry runner runs it: in `dir`, inheriting the
    /// caller's environment while keeping the adapter's fixed variables.
    pub(crate) fn client_command(&self, dir: &Path) -> Command {
        let mut command = self.command();
        command.current_dir(dir);
        for &(name, value) in self.adapter.fixed {
            command.env(name, value);
        }
        command
    }
}

/// The variables the steps of any tool may set.
pub(crate) fn settable() -> Vec<String> {
    (ADAPTERS.iter())
        .flat_map(|a| a.settable)
        .map(|&name| name.to_owned())
        .collect()
}

/// A step sets only the variables of its own tool's adapter.
pub(crate) fn authorize(step: &ToolchainStep) -> Result<(), String> {
    let adapter = (ADAPTERS.iter())
        .find(|a| a.id == step.toolchain.id)
        .ok_or_else(|| {
            format!(
                "the host has no adapter for the tool `{}`",
                step.toolchain.id
            )
        })?;
    match (step.environment.iter()).find(|v| !adapter.settable.contains(&v.key.as_str())) {
        Some(v) => Err(format!(
            "step `{}` sets `{}`, which `{}` does not take",
            step.id, v.key, adapter.id
        )),
        None => Ok(()),
    }
}

/// A launch passes only runtime arguments and environment variables approved
/// by its runtime's host adapter.
pub(crate) fn authorize_launch(launch: &Launch) -> Result<(), String> {
    authorize_launch_parts(
        launch.runtime.as_ref(),
        &launch.runtime_arguments,
        &launch.environment,
    )
}

pub(crate) fn authorize_launch_parts(
    runtime: Option<&Toolchain>,
    runtime_arguments: &[String],
    environment: &[OptionValue],
) -> Result<(), String> {
    let Some(runtime) = runtime else {
        if !runtime_arguments.is_empty() {
            return Err("launch without a runtime cannot pass runtime arguments".into());
        }
        if let Some(var) = environment.first() {
            return Err(format!(
                "launch without a runtime cannot set environment variable `{}`",
                var.key
            ));
        }
        return Ok(());
    };
    let adapter = (ADAPTERS.iter())
        .find(|a| a.id == runtime.id)
        .ok_or_else(|| format!("the host has no adapter for the tool `{}`", runtime.id))?;
    (adapter.validate_launch_args)(runtime_arguments)?;
    match (environment.iter()).find(|v| !adapter.launch_settable.contains(&v.key.as_str())) {
        Some(v) => Err(format!(
            "launch sets `{}`, which `{}` does not take",
            v.key, adapter.id
        )),
        None => Ok(()),
    }
}

fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use rustix::fs::{Access, AtFlags, CWD, accessat};
        accessat(CWD, path, Access::EXEC_OK, AtFlags::EACCESS).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(all(test, unix))]
#[path = "tools_tests.rs"]
pub(super) mod tests;
