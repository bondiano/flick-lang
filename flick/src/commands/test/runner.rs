//! A test launch and the shared reporting and cancellation arguments.

use crate::args::{Color, GlobalArguments, Reporter, TestArguments};
#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::backends::Selected;
use crate::progress::Mode;
use flick_core::project::suite_name;
use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;
use std::io::{self, IsTerminal};
use std::path::{Display, Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The manifest of the snapshot files, beside the runner.
pub(super) const SNAPSHOTS: &str = "snapshots.json";

/// The manifest of the snapshot files of the test modules `modules`, by
/// their names and source paths: a JSON object whose keys are the module
/// names the runner gives `runTests` and whose values are the paths of
/// their files, `__snapshots__/<File>.snap` beside their sources, relative to
/// the manifest in `out`, `out` relative to the project's `root`. Runtimes
/// read it from `--snapshots` (see `runtime/src/core/snapshots.ts`).
pub(super) fn snapshot_manifest<'a>(
    root: &Path,
    out: &str,
    modules: impl Iterator<Item = (&'a str, &'a Path)>,
) -> String {
    let up = "../".repeat(Path::new(out).components().count());
    let files: BTreeMap<&str, String> = modules
        .filter_map(|(name, source)| {
            let stem = source.file_name()?.to_str()?.strip_suffix(".fl")?;
            let file = source.parent()?.join("__snapshots__").join(format!("{stem}.snap"));
            let path = match file.strip_prefix(root) {
                Ok(inside) => format!("{up}{}", inside.to_string_lossy().replace('\\', "/")),
                Err(_) => file.to_string_lossy().into_owned(),
            };
            Some((suite_name(name), path))
        })
        .collect();
    serde_json::to_string(&files).unwrap_or_default()
}

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
        .map(|(i, (name, _))| format!("[{:?}, $s{i}.tests]", suite_name(name)))
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
    if options.list {
        command.arg("--list");
    }
    if options.forbid_only {
        command.arg("--forbid-only");
    }
    if runner.snapshots.is_file() {
        command.arg("--snapshots").arg(&runner.snapshots);
    }
    if options.update_snapshots {
        command.arg("--update-snapshots");
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

pub(super) struct Runner {
    path: PathBuf,
    /// The manifest of the snapshot files.
    snapshots: PathBuf,
    #[cfg(feature = "wasm-backends")]
    backend: Option<flick_backend::BackendPin>,
}

impl Runner {
    pub(super) fn native(path: PathBuf) -> Self {
        Self {
            snapshots: path.with_file_name(SNAPSHOTS),
            path,
            #[cfg(feature = "wasm-backends")]
            backend: None,
        }
    }
    pub(super) fn component(out: PathBuf, selected: &Selected) -> Self {
        #[cfg(not(feature = "wasm-backends"))]
        let _ = selected;
        Self {
            snapshots: out.join(SNAPSHOTS),
            path: out,
            #[cfg(feature = "wasm-backends")]
            backend: selected.pin(),
        }
    }
    pub(super) fn display(&self) -> Display<'_> {
        self.path.display()
    }
}
