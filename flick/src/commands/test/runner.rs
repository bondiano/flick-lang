//! A test launch and the shared reporting and cancellation arguments.

use crate::args::{Color, GlobalArguments, Reporter, TestArguments};
#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::progress::Mode;
use flick_core::project::Project;
use std::env;
use std::fmt::Write;
use std::io::{self, IsTerminal};
use std::path::{Display, PathBuf};
use std::process::{Child, Command, Stdio};

/// The runner: imports each suite's `tests` and runs them all, on Flick's own
/// runtime with `rt`.
pub(super) fn runner_source(suites: &[(String, String)], rt: bool) -> String {
    let runtime = if rt {
        "@flick-lang/runtime/rt"
    } else {
        "@flick-lang/effect-ts"
    };
    let mut out = format!("import {{ runTests }} from \"{runtime}\";\n");
    for (i, (_, path)) in suites.iter().enumerate() {
        let _ = writeln!(out, "import * as $s{i} from {path:?};");
    }
    let list: Vec<String> = suites
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!("[{name:?}, $s{i}.tests]"))
        .collect();
    let _ = write!(out, "\nawait runTests([{}]);\n", list.join(", "));
    out
}

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

pub(super) fn runner_command(
    runner: &Runner,
    options: &TestArguments,
    global: &GlobalArguments,
    watched: bool,
) -> Result<Command, String> {
    // the runner is the test executable of the go target, or a script for node
    #[cfg(feature = "wasm-backends")]
    let selected = runner.backend.as_ref().map(|pin| {
        backends::launch::command(&runner.path, pin, flick_backend::LaunchKind::Test, "test")
    });
    #[cfg(not(feature = "wasm-backends"))]
    let selected: Option<Result<Command, String>> = None;
    let mut command = if let Some(command) = selected {
        command?
    } else if runner.path.extension().is_some_and(|e| e == "js") {
        let mut command = Command::new("node");
        command.arg("--enable-source-maps").arg(&runner.path);
        command
    } else {
        Command::new(&runner.path)
    };
    if watched {
        command.arg("--watched").stdin(Stdio::piped());
    }
    if let Some(filter) = &options.filter {
        command.args(["--filter", filter]);
    }
    command.args(["--jobs", &options.jobs.to_string()]);
    if let Some(seed) = options.seed {
        command.args(["--seed", &seed.to_string()]);
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

pub(super) struct Runner {
    path: PathBuf,
    #[cfg(feature = "wasm-backends")]
    backend: Option<flick_backend::BackendPin>,
}

impl Runner {
    pub(super) fn native(path: PathBuf) -> Self {
        Self {
            path,
            #[cfg(feature = "wasm-backends")]
            backend: None,
        }
    }
    pub(super) fn component(out: PathBuf, project: &Project) -> Self {
        #[cfg(not(feature = "wasm-backends"))]
        let _ = project;
        Self {
            path: out,
            #[cfg(feature = "wasm-backends")]
            backend: project.config.build.backend.as_ref().map(backends::pin),
        }
    }
    pub(super) fn display(&self) -> Display<'_> {
        self.path.display()
    }
}
