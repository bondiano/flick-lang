//! Running package documentation examples: the code a page shows executes,
//! against the package's own dependencies, and a failing example fails the
//! documentation gate.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command, Stdio};
use std::slice;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::symlink as unix_symlink;

use flick_core::build::{self, Backend, Extras, Settings};
use flick_core::codegen::EffectBridge;
use flick_core::diagnostics::Severity;
use flick_core::markdown::{self, Mode};
use flick_core::project::{self, Analysis, Project};
use flick_core::source::{FileId, SourceMap};
use flick_core::types::builtin_sources;

use super::Checked;
use super::example_host::ExampleHost;

/// How long an example may run before the gate calls it hung.
const TIMEOUT: Duration = Duration::from_mins(1);

/// Runs a ```` ```flick ```` block that exports `main`, if it is one the page
/// expects to succeed; the message of what went wrong, without the prefix the
/// caller adds.
pub(super) fn check_example(checked: &Checked, block: &markdown::Block) -> Option<String> {
    // `fail` and `ignore` blocks are not expected to run; `keep` blocks are
    // displayed but hidden lines are not part of the example's program
    if block.mode != Mode::Check {
        return None;
    }
    let root = checked
        .project
        .as_ref()
        .map_or_else(|| checked.host.cwd.clone(), Project::source_root);
    let declared = block.text.lines().find_map(|line| {
        line.strip_prefix("module ")
            .and_then(|s| s.split_whitespace().next())
    });
    let name = declared.unwrap_or("DocExample");
    let text = if declared.is_some() {
        block.text.clone()
    } else {
        format!("module {name} exposing (..)\n{}", block.text)
    };
    let path = root.join(project::module_path(name));
    let host = ExampleHost::new(&checked.host, path.clone(), text);
    let mut sources = SourceMap::default();
    let analysis = project::analyze_rt(
        &host,
        &mut sources,
        checked.project.as_ref(),
        slice::from_ref(&path),
    );
    let errors: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        return Some(format!(
            "line {} did not compile: {}",
            block.line, errors[0].message
        ));
    }
    let module = (analysis.compilation.modules.iter()).find(|m| m.path == path)?;
    // a page's module without `main` is a library: it has nothing to run
    if !module.interface.exports_value("main") {
        return None;
    }
    run(checked, block, &analysis, &sources, &path, &module.name)
}

/// Executes one compiled example and reports a runtime failure.
fn run(
    checked: &Checked,
    block: &markdown::Block,
    analysis: &Analysis,
    sources: &SourceMap,
    path: &Path,
    name: &str,
) -> Option<String> {
    let effect = if checked.project.as_ref().is_none_or(|p| p.config.js_is_rt()) {
        Backend::Rt
    } else {
        Backend::Effect
    };
    let entries = [("main".to_owned(), name.to_owned())];
    let source_path = |_: FileId| format!("file://{}", path.display());
    let source_dir = |_: &str| path.parent().map(Path::to_path_buf);
    let out = Path::new(".");
    let settings = Settings {
        package: None,
        entries: &entries,
        typescript: false,
        release: false,
        extras: Extras::default(),
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: out,
        root: None,
        backend: effect,
        effect_bridge: EffectBridge::Available,
        ui: None,
        runners: &[],
        environment: None,
    };
    let (artifacts, problems) = build::emit(&analysis.compilation, sources, &settings);
    let errors: Vec<_> = problems
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        return Some(format!(
            "line {} did not build: {}",
            block.line, errors[0].message
        ));
    }
    let dir = match TempDir::new("doc-example") {
        Ok(dir) => dir,
        Err(err) => return Some(format!("line {} cannot build: {err}", block.line)),
    };
    for artifact in artifacts {
        let file = dir.path().join(&artifact.path);
        if let Some(parent) = file.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            return Some(format!("line {} cannot build: {err}", block.line));
        }
        if let Err(err) = fs::write(&file, artifact.contents) {
            return Some(format!("line {} cannot build: {err}", block.line));
        }
    }
    // the runtime packages an effectful example imports resolve as in a build
    if let Some(project) = checked.project.as_ref()
        && let Some(modules) = node_modules(project)
    {
        let _ = symlink(&modules, &dir.path().join("node_modules"));
    }
    let runner = dir.path().join(build::runner_path(false));
    let mut command = Command::new("node");
    command
        .arg("--enable-source-maps")
        .arg(&runner)
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return Some(format!(
                "line {} failed at runtime: cannot start `node`: {err}",
                block.line
            ));
        }
    };
    match wait(child) {
        Ok((true, _)) => None,
        Ok((false, stderr)) => {
            let stderr = stderr.trim();
            let stderr = if stderr.is_empty() {
                "a non-zero exit"
            } else {
                stderr
            };
            Some(format!("line {} failed at runtime: {stderr}", block.line))
        }
        Err(err) => Some(format!("line {} failed at runtime: {err}", block.line)),
    }
}

/// Waits for the example; `true` when it exited successfully.
fn wait(mut child: Child) -> io::Result<(bool, String)> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            let output = child.wait_with_output()?;
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Ok((status.success(), stderr));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok((
                false,
                "the example did not finish within 60 seconds".to_owned(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// The first imported module root that the project and the standard library
/// do not provide and that no declared dependency provides from its source
/// directory, so the example imports a package `flick.toml` does not declare.
pub(super) fn undeclared_import(checked: &Checked, text: &str) -> Option<String> {
    checked.project.as_ref()?;
    let mut known: HashSet<&str> = checked
        .compilation
        .modules
        .iter()
        .filter_map(|m| m.name.split('.').next())
        .collect();
    for (name, _) in builtin_sources() {
        if let Some(root) = name.split('.').next() {
            known.insert(root);
        }
    }
    // a dependency provides the modules under its source directory: a
    // `Foo.Bar` import needs the directory `Foo` there
    let libraries: Vec<&Path> = checked
        .compilation
        .libraries
        .iter()
        .map(|(_, dir)| dir.as_path())
        .collect();
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix("import ") else {
            continue;
        };
        let root = rest
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("");
        if root.is_empty() || known.contains(root) {
            continue;
        }
        if libraries.iter().any(|dir| dir.join(root).exists()) {
            continue;
        }
        return Some(root.to_owned());
    }
    None
}

/// The `node_modules` directory of the package or the workspace around it.
fn node_modules(project: &Project) -> Option<PathBuf> {
    let mut dirs = vec![project.root.clone()];
    dirs.extend(project.workspace.clone());
    for dir in dirs {
        for at in dir.ancestors() {
            let modules = at.join("node_modules");
            if modules.is_dir() {
                return Some(modules);
            }
        }
    }
    None
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> io::Result<()> {
    unix_symlink(from, to)
}

#[cfg(not(unix))]
fn symlink(from: &Path, to: &Path) -> io::Result<()> {
    let _ = (from, to);
    Err(io::Error::new(io::ErrorKind::Unsupported, "no symlinks"))
}

/// A directory removed when it goes out of scope.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> io::Result<Self> {
        let path = env::temp_dir().join(format!("flick-{tag}-{}-{}", process::id(), nanos()));
        fs::create_dir_all(&path)?;
        Ok(TempDir { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}
