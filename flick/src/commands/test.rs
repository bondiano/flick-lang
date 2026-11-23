//! `flick test`: builds the project with its test root into `.flick/test/`
//! through the selected target backend and runs its test launch.
//! With `--watch`, a change of a source file stops the run (closing the
//! runner's stdin: every running test closes its scope) and starts the next
//! one after it.

use std::path::{Path, PathBuf};
use std::process::{Child, ExitCode};
use std::time::{Duration, SystemTime};
use std::{fs, thread};

use crate::artifact::{Artifact, resolve_root, validate_root};
use flick_backend::Verb;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{self, ModuleCache, Project};
use flick_core::source::SourceMap;

use super::build::{BuildPlan, Purpose, artifacts, publish_artifacts, release, target_override};
use super::check::select;
use crate::args::{GlobalArguments, Reporter, TestArguments};
use crate::backends::options::{self, ResolvedOptions};
use crate::backends::{self, Selected};
use crate::package_cache;
use crate::progress::{Mode, Progress};
use crate::sources::FsHost;
use crate::{builtins, cache, child, report, sources, timings};
use flick_core::build::Extras;

mod cached;
mod runner;
use cached::cached_prepare;
use runner::{Runner, runner_command, start};

/// Where the test build goes, inside the project.
const OUT_DIR: &str = ".flick/test";

pub fn test(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    if options.watch {
        return watch(options, global);
    }
    let runner = match prepare(options, global, None) {
        Ok(runner) => runner,
        Err(code) => return ExitCode::from(code),
    };
    let signals = child::signals();
    let mut command = match runner_command(&runner, options, global, false) {
        Ok(command) => command,
        Err(why) => {
            report::render(global, &SourceMap::default(), &[backends::failure(why)]);
            return ExitCode::from(2);
        }
    };
    match child::spawn(&mut command, global, "application-stderr").and_then(|r| r.wait(&signals)) {
        Ok(status) => ExitCode::from(child::code(status)),
        Err(err) => {
            eprintln!("error: cannot run `{}`: {err}", runner.display());
            ExitCode::from(2)
        }
    }
}

/// Builds the test run and writes its runner.
#[allow(clippy::too_many_lines)]
fn prepare(
    options: &TestArguments,
    global: &GlobalArguments,
    modules: Option<&mut ModuleCache>,
) -> Result<Runner, u8> {
    let progress = Progress::new(if matches!(options.reporter, Reporter::Json) {
        Mode::Hidden
    } else {
        Mode::for_cli(global)
    });
    let mut sources = SourceMap::default();
    let host = FsHost::new();
    let fail = |sources: &SourceMap, diagnostics: Vec<Diagnostic>| {
        let code = report::code(&diagnostics).max(1);
        report::render(global, sources, &diagnostics);
        Err(code)
    };
    progress.stage("Resolving project");
    let (project, roots) = match select(&host, &mut sources, global.project.as_deref(), &[]) {
        Ok(selected) => selected,
        Err(diagnostic) => {
            drop(progress);
            return fail(&sources, vec![diagnostic]);
        }
    };
    let Some(project) = project else {
        return Err(1);
    };
    // the test companions beside the sources, which only this command runs
    let mut roots = roots;
    let mut companions = Vec::new();
    let skip = sources::skip_dirs(Some(&project));
    if project.source_root().is_dir() {
        sources::companions_skipping(
            &project.source_root().to_string_lossy(),
            &skip,
            &mut companions,
        );
    }
    roots.extend(companions.into_iter().map(PathBuf::from));
    let wanted: Vec<PathBuf> = options
        .paths
        .iter()
        .map(|p| project::normalize(&host.cwd.join(p)))
        .collect();
    let mut selected = wanted.clone();
    selected.sort();
    selected.dedup();
    let (backend, choice, target_overridden) =
        match Selected::load_target(&host, Some(&project), options.target.as_deref()) {
            Ok(loaded) => loaded,
            Err(diagnostic) => {
                drop(progress);
                return fail(&sources, vec![diagnostic]);
            }
        };
    let descriptor = backend.descriptor();
    let resolved = match &descriptor {
        Some(descriptor) => match options::resolve(
            descriptor,
            Verb::Test,
            Some(&project),
            &options.backend_options,
        ) {
            Ok(resolved) => resolved,
            Err(diagnostics) => {
                drop(progress);
                return fail(&sources, diagnostics);
            }
        },
        None => ResolvedOptions::default(),
    };
    let stages_by_class = options::stages_by_class(descriptor.as_ref(), &resolved);
    if let Some(problem) = backends::check_runtime(&project, &backend, &resolved) {
        drop(progress);
        return fail(&sources, vec![problem]);
    }
    let identity = backend.cache_identity(&resolved);
    let settings = format!(
        "test {:?} {} {:?} {} {identity}",
        selected,
        choice.short(),
        release(options.profile, Some(&project.config.build)),
        resolved.cache_key(),
    );
    let key = cache::key(
        &project,
        &roots,
        &settings,
        options.target.as_deref(),
        &resolved,
    );
    let out = resolve_root(&project.root, &project.root.join(OUT_DIR));
    if let Err(why) = validate_root(&out) {
        drop(progress);
        return fail(
            &sources,
            vec![Diagnostic::new(codes::P013, Severity::Error, why)],
        );
    }
    progress.stage("Checking test cache");
    if let Some(runner) = cached_prepare(&project, &key, &out, &progress, &backend) {
        return Ok(runner);
    }
    progress.stage("Preparing compiler");
    if let Err(diagnostic) = builtins::prepare_in(stages_by_class) {
        drop(progress);
        return fail(&sources, vec![diagnostic]);
    }
    progress.stage("Compiling Flick modules");
    let mut override_cfg = target_override(
        &backend,
        &choice,
        target_overridden,
        &resolved,
        stages_by_class,
        true,
        wanted.clone(),
    );
    override_cfg.skip_backend_check_target = true;
    let (mut diagnostics, mut compilation) = package_cache::compile_for(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&override_cfg),
        modules,
    );
    super::build::hooks::ir(Some(&project), &mut compilation, &mut diagnostics);
    let errors = diagnostics.iter().any(|d| {
        d.severity == Severity::Error
            || (global.warnings_as_errors && d.severity == Severity::Warning)
    });
    let progress = if diagnostics.is_empty() {
        progress
    } else {
        drop(progress);
        report::render(global, &sources, &diagnostics);
        if errors {
            return Err(report::code(&diagnostics).max(1));
        }
        Progress::new(if matches!(options.reporter, Reporter::Json) {
            Mode::Hidden
        } else {
            Mode::for_cli(global)
        })
    };
    let suites: Vec<String> = compilation
        .modules
        .iter()
        .filter(|m| m.test && m.interface.exports_value("tests"))
        .filter(|m| wanted.is_empty() || wanted.contains(&m.path))
        .filter(|m| compilation.module_available(&m.name))
        .map(|m| m.name.clone())
        .collect();
    let output = BuildPlan {
        purpose: Purpose::Test,
        backend: backend.clone(),
        target: choice.short().to_owned(),
        target_overridden,
        options: resolved.clone(),
        inspect_module: None,
        out_dir: out.clone(),
        entries: Vec::new(),
        typescript: false,
        stages_by_class,
        suites,
        extras: Extras {
            rt: stages_by_class,
            ..Extras::default()
        },
        release: release(options.profile, Some(&project.config.build)),
    };
    progress.stage("Emitting backend output");
    let mut emitted = artifacts(&host, &mut sources, Some(&project), &compilation, &output);
    if global.warnings_as_errors {
        emitted.diagnostics = report::promote(&emitted.diagnostics);
    }
    publish_artifacts(&output, &mut emitted, &progress, Some(&project.root));
    let progress = if emitted.diagnostics.is_empty() {
        progress
    } else {
        drop(progress);
        report::render(global, &sources, &emitted.diagnostics);
        if emitted.diagnostics.iter().any(|d| {
            d.severity == Severity::Error
                || (global.warnings_as_errors && d.severity == Severity::Warning)
        }) {
            return Err(report::code(&emitted.diagnostics).max(1));
        }
        Progress::new(if matches!(options.reporter, Reporter::Json) {
            Mode::Hidden
        } else {
            Mode::for_cli(global)
        })
    };
    let read = (&roots[..], &*host.cwd, &sources);
    let store = |artifacts: &[Artifact]| {
        let now = cache::key(
            &project,
            &roots,
            &settings,
            options.target.as_deref(),
            &resolved,
        );
        diagnostics.is_empty()
            && emitted.diagnostics.is_empty()
            && cache::store_built(
                &project,
                read,
                options.target.as_deref(),
                &key,
                &now,
                artifacts,
            )
    };
    store(&emitted.artifacts);
    drop(progress);
    Ok(Runner::component(out, &backend))
}

/// The source and test roots and the configuration of the project.
pub fn watched(global: &GlobalArguments) -> Vec<PathBuf> {
    let host = FsHost::new();
    let explicit = global.project.as_deref().map(Path::new);
    match project::discover(&host, &mut SourceMap::default(), &host.cwd, explicit) {
        Ok(Some(project)) => watched_roots(&project),
        _ => vec![host.cwd],
    }
}

/// What a watch of `project` follows: its roots, its configuration and the
/// libraries it depends on by path.
pub fn watched_roots(project: &Project) -> Vec<PathBuf> {
    // shortcut: only direct path dependencies are watched, add theirs when a workspace nests libraries
    let libraries = project
        .config
        .dependencies
        .iter()
        .filter_map(|d| Some(project::normalize(&project.root.join(d.path()?))));
    [
        project.source_root(),
        project.test_root(),
        project.root.join(project::CONFIG_FILE),
    ]
    .into_iter()
    .chain(libraries)
    .collect()
}

/// Per-path filesystem state, independent of the newest timestamp in the tree.
#[derive(Debug, PartialEq, Eq)]
pub struct WatchSnapshot(Vec<WatchedPath>);

#[derive(Debug, PartialEq, Eq)]
struct WatchedPath {
    path: PathBuf,
    modified: Option<SystemTime>,
    size: u64,
    directory: bool,
}

/// Captures changes, additions and removals even when another file is future-dated.
pub fn watch_snapshot(paths: &[PathBuf]) -> WatchSnapshot {
    fn capture(path: &Path, out: &mut Vec<WatchedPath>) {
        let Ok(meta) = fs::metadata(path) else {
            return;
        };
        out.push(WatchedPath {
            path: path.to_path_buf(),
            modified: meta.modified().ok(),
            size: meta.len(),
            directory: meta.is_dir(),
        });
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                capture(&entry.path(), out);
            }
        }
    }
    let mut out = Vec::new();
    for path in paths {
        capture(path, &mut out);
    }
    out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    WatchSnapshot(out)
}

/// The snapshot once two in a row agree: a save that truncates a file and
/// then writes it is one change, not a build of each state. A file that
/// keeps changing (a log in a watched directory) ends the wait after about
/// a second, with the latest snapshot.
pub fn settled_snapshot(paths: &[PathBuf]) -> WatchSnapshot {
    settle(paths, || thread::sleep(Duration::from_millis(50)))
}

fn settle(paths: &[PathBuf], mut pause: impl FnMut()) -> WatchSnapshot {
    let mut last = watch_snapshot(paths);
    for _ in 0..20 {
        pause();
        let now = watch_snapshot(paths);
        if now == last {
            break;
        }
        last = now;
    }
    last
}

/// Stops a run: closing its stdin lets every running test close its scope,
/// and the wait returns only once the runner has exited.
fn stop(child: &mut Child) {
    drop(child.stdin.take());
    let _ = child.wait();
}

fn watch(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    let paths = watched(global);
    // what the runs of this session compiled: a change compiles what it reaches
    let mut modules = ModuleCache::default();
    loop {
        let before = settled_snapshot(&paths);
        let prepared = prepare(options, global, Some(&mut modules));
        // each run reports its build, before its tests start
        timings::report(global);
        let mut child = prepared
            .ok()
            .and_then(|runner| start(&runner, options, global, true).ok());
        while watch_snapshot(&paths) == before {
            thread::sleep(Duration::from_millis(200));
        }
        if let Some(child) = &mut child {
            stop(child);
        }
        timings::discard();
        println!("\n— a file changed: running the tests again —\n");
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs::{File, FileTimes};
    use std::process;
    use std::time::UNIX_EPOCH;

    use super::*;

    #[test]
    fn snapshots_follow_each_path_despite_future_timestamps() {
        let root = env::temp_dir().join(format!("flick-watch-snapshot-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("fixture");
        let future = root.join("Future.fl");
        let ordinary = root.join("Main.fl");
        fs::write(&future, "future").expect("future");
        fs::write(&ordinary, "1").expect("main");
        let set_time = |path: &Path, seconds| {
            File::open(path)
                .expect("file")
                .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
                .expect("timestamp");
        };
        set_time(&future, 4_102_444_800);
        set_time(&ordinary, 1_000);
        let paths = [root.clone()];
        let before = watch_snapshot(&paths);
        assert_eq!(before, watch_snapshot(&paths), "stable order and state");
        fs::write(&ordinary, "2").expect("change");
        set_time(&ordinary, 2_000);
        let changed = watch_snapshot(&paths);
        assert_ne!(before, changed, "the future file cannot mask another file");
        let created = root.join("New.fl");
        fs::write(&created, "new").expect("create");
        set_time(&created, 1_000);
        let added = watch_snapshot(&paths);
        assert_ne!(changed, added, "creation changes path identity");
        fs::remove_file(created).expect("delete");
        assert_ne!(
            added,
            watch_snapshot(&paths),
            "deletion changes path identity"
        );
        fs::remove_dir_all(&root).expect("delete root");
        assert_eq!(watch_snapshot(&paths), WatchSnapshot(Vec::new()));
    }

    #[test]
    fn a_save_settles_on_the_saved_file_and_a_busy_file_ends_the_wait() {
        let root = env::temp_dir().join(format!("flick-watch-settle-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("fixture");
        let main = root.join("Main.fl");
        fs::write(&main, "old").expect("old");
        let paths = [root.clone()];
        // a save has truncated the file and writes it during the first pause
        File::create(&main).expect("truncate");
        let mut saved = false;
        let mut save = || {
            if !saved {
                fs::write(&main, "saved").expect("write");
                saved = true;
            }
        };
        let settled = settle(&paths, &mut save);
        save();
        assert_eq!(settled, watch_snapshot(&paths), "the saved file");
        let mut writes = 0;
        let busy = settle(&paths, || {
            writes += 1;
            fs::write(&main, "x".repeat(writes)).expect("write");
        });
        assert_eq!(writes, 20, "the wait is bounded");
        assert_eq!(busy, watch_snapshot(&paths), "the latest state");
        fs::remove_dir_all(&root).expect("delete root");
    }
}
