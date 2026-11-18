//! Host tools that build steps and launches run. A tool is found on `PATH`
//! and pinned by the digest and version of its executable; a step runs it
//! with a cleared environment that keeps only what its adapter names.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

use flick_backend::{Toolchain, ToolchainStep};
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
        let name = if cfg!(windows) {
            format!("{id}.exe")
        } else {
            id.to_owned()
        };
        let path = env::split_paths(paths)
            .map(|dir| dir.join(&name))
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
        let output = Command::new(&path)
            .args(adapter.version)
            .output()
            .map_err(|e| e.to_string())?;
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
