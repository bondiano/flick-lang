//! A test launch and the shared reporting and cancellation arguments.

#[cfg(feature = "wasm-backends")]
use crate::args::{Color, Reporter};
use crate::args::{GlobalArguments, TestArguments};
#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::backends::Selected;
#[cfg(feature = "wasm-backends")]
use crate::progress::Mode;
#[cfg(feature = "wasm-backends")]
use std::env;
use std::io;
#[cfg(feature = "wasm-backends")]
use std::io::IsTerminal;
use std::path::{Display, PathBuf};
#[cfg(feature = "wasm-backends")]
use std::process::Stdio;
use std::process::{Child, Command};

pub(super) fn start(
    runner: &Runner,
    options: &TestArguments,
    global: &GlobalArguments,
    watched: bool,
) -> io::Result<Child> {
    runner_command(runner, options, global, watched)
        .map_err(io::Error::other)?
        .spawn()
}

#[cfg(feature = "wasm-backends")]
pub(super) fn runner_command(
    runner: &Runner,
    options: &TestArguments,
    global: &GlobalArguments,
    watched: bool,
) -> Result<Command, String> {
    let mut command = match runner.backend.as_ref() {
        Some(pin) => {
            backends::launch::command(&runner.path, pin, flick_backend::LaunchKind::Test, "test")?
        }
        None => return Err("running tests requires WebAssembly backend support".into()),
    };
    if watched {
        command.arg("--watched").stdin(Stdio::piped());
    }
    if let Some(filter) = &options.filter {
        command.args(["--filter", filter]);
    }
    if options.list {
        command.arg("--list");
    }
    if options.forbid_only {
        command.arg("--forbid-only");
    }
    command.args(["--jobs", &options.jobs.to_string()]);
    if let Some(seed) = options.seed {
        command.args(["--seed", &seed.to_string()]);
    }
    if let Some(timeout) = options.timeout {
        command.args(["--timeout", &timeout.to_string()]);
    }
    let reporter = match options.reporter {
        Reporter::Human => "human",
        Reporter::Json => "json",
    };
    command.args(["--reporter", reporter]);
    if matches!(options.reporter, Reporter::Human)
        && matches!(Mode::for_cli(global), Mode::Live { .. })
        && io::stdout().is_terminal()
    {
        command.args(["--progress", "live"]);
    }
    let color = matches!(options.reporter, Reporter::Human)
        && match global.color {
            Color::Always => true,
            Color::Never => false,
            Color::Auto => {
                io::stdout().is_terminal()
                    && env::var_os("NO_COLOR").is_none()
                    && env::var_os("TERM").is_none_or(|term| term != "dumb")
            }
        };
    command.args(["--color", if color { "always" } else { "never" }]);
    Ok(command)
}

#[cfg(not(feature = "wasm-backends"))]
pub(super) fn runner_command(
    _runner: &Runner,
    _options: &TestArguments,
    _global: &GlobalArguments,
    _watched: bool,
) -> Result<Command, String> {
    Err("running tests requires WebAssembly backend support".into())
}

pub(super) struct Runner {
    path: PathBuf,
    #[cfg(feature = "wasm-backends")]
    backend: Option<flick_backend::BackendPin>,
}

impl Runner {
    pub(super) fn component(out: PathBuf, selected: &Selected) -> Self {
        #[cfg(not(feature = "wasm-backends"))]
        let _ = selected;
        Self {
            path: out,
            #[cfg(feature = "wasm-backends")]
            backend: selected.pin(),
        }
    }
    pub(super) fn display(&self) -> Display<'_> {
        self.path.display()
    }
}
