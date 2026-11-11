//! E2E harness: builds each `tests/run/**/<name>.fl` with `flick build`, runs
//! the runner in Node with source maps and compares stdout, stderr and exit
//! code with `<name>.stdout` (required), `<name>.stderr` (default empty) or
//! `<name>.stderr.contains` (lines stderr must contain), and `<name>.exit`
//! (default `0`). Cases run in parallel, `FLICK_E2E_JOBS` at a time
//! (default: the number of CPUs). `FLICK_E2E_ONLY` runs the cases whose path
//! contains it, or one of its comma-separated parts.
//!
//! The cases `tests/run/portable.txt` lists also run on the go target in
//! both profiles and must give the same output; the others are listed as
//! skipped for it. `FLICK_E2E_GO_ONLY` runs only those, without Node: the
//! platform jobs of CI.
//!
//! `FLICK_E2E_TS_BUILD_ONLY` builds the TypeScript passes and typechecks
//! them without running them: what the TypeScript output catches beyond the
//! JS one is `tsc`, so the pull requests and pushes to main of CI skip those
//! runs, and the nightly run and a release keep them.
//!
//! The cases `tests/run/known_gaps.txt` lists for a target are a law that
//! target does not hold yet: they must fail there, and one that passes is
//! reported, so its line goes and the case holds the target from then on.
//!
//! Each pass prints how long it took, and `times.tsv` under the output
//! directory keeps the build and run time of every case (`times`).
//! `FLICK_E2E_SHARD=k/n` runs the k-th of n shards of the cases, every pass
//! of each, for CI jobs side by side.
//!
//! Every run writes its trace (`FLICK_TRACE`), which the monitor of `λRT`
//! checks; for the cases of tests/run/runtime the Go runs must project it
//! to what the JS debug run projects it to (`trace`).

mod gaps;
mod rt;
mod times;
mod trace;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use flick_core::build::go::executable;
use flick_core::build::runner_path;
use flick_core::project::config::MAIN_ENTRY;
use times::{Pass, Timing};

fn cases(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable tests/run") {
        let path = entry.expect("readable entry").path();
        if path.is_dir() {
            cases(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

fn expected(case: &Path, ext: &str) -> Option<String> {
    fs::read_to_string(case.with_extension(ext)).ok()
}

fn compare(what: &str, expected: &str, actual: &str) -> Option<String> {
    (expected != actual)
        .then(|| format!("  {what}:\n    expected: {expected:?}\n    actual:   {actual:?}"))
}

/// A case run: the mismatch report, if any, how long the build and the
/// program took, and the projection of its trace when the runtimes must
/// agree on it.
struct Ran {
    problem: Option<String>,
    built: Duration,
    executed: Duration,
    projection: Option<String>,
}

impl Ran {
    fn timing(&self, rel: &Path) -> Timing {
        Timing {
            case: rel.to_path_buf(),
            built: self.built,
            ran: self.executed,
        }
    }
}

/// Builds and runs one case, as JS, TypeScript or Go, in a profile.
fn run_case(case: &Path, rel: &Path, out_dir: &Path, emit: &str, profile: &str) -> Ran {
    run_case_with(case, rel, out_dir, emit, profile, &[], None)
}

/// [`run_case`], with `flags` for the build, and a program stopped after `limit`.
fn run_case_with(
    case: &Path,
    rel: &Path,
    out_dir: &Path,
    emit: &str,
    profile: &str,
    flags: &[&str],
    limit: Option<Duration>,
) -> Ran {
    let built = build_case(case, out_dir, emit, profile, flags);
    if built.problem.is_some() {
        return built;
    }
    let mut command = if emit == "go" {
        Command::new(out_dir.join(executable(MAIN_ENTRY)))
    } else {
        let mut node = Command::new("node");
        node.arg("--enable-source-maps")
            .arg(out_dir.join(runner_path(emit == "ts")));
        node
    };
    let started = Instant::now();
    let output = rt::output_within(command.env("FLICK_TRACE", out_dir.join(trace::FILE)), limit);
    let run_time = started.elapsed();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_problem = match expected(case, "stderr.contains") {
        Some(lines) => lines
            .lines()
            .find(|line| !stderr.contains(line))
            .map(|line| format!("  stderr:\n    lacks:  {line:?}\n    actual: {stderr:?}")),
        None => compare(
            "stderr",
            &expected(case, "stderr").unwrap_or_default(),
            &stderr,
        ),
    };
    let traced = trace::checked(out_dir);
    let monitor = traced
        .as_ref()
        .err()
        .map(|violation| format!("  λRT: {violation}"));
    let problems: Vec<_> = [
        compare(
            "stdout",
            &expected(case, "stdout").expect("`<name>.stdout` exists"),
            &String::from_utf8_lossy(&output.stdout),
        ),
        stderr_problem,
        compare(
            "exit",
            expected(case, "exit").as_deref().unwrap_or("0").trim(),
            &output
                .status
                .code()
                .map_or("signal".into(), |c| c.to_string()),
        ),
        monitor,
    ]
    .into_iter()
    .flatten()
    .collect();
    Ran {
        problem: (!problems.is_empty()).then(|| problems.join("\n")),
        built: built.built,
        executed: run_time,
        projection: traced.ok().filter(|_| trace::compared(rel)),
    }
}

/// Builds one case into `out_dir`, without running it: a problem only when
/// the build fails.
fn build_case(case: &Path, out_dir: &Path, emit: &str, profile: &str, flags: &[&str]) -> Ran {
    let _ = fs::remove_dir_all(out_dir);
    let started = Instant::now();
    let build = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .arg(case)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--emit")
        .arg(emit)
        .arg("--profile")
        .arg(profile)
        .args(flags)
        .output()
        .expect("flick runs");
    let problem = (!build.status.success()).then(|| {
        format!(
            "  build failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        )
    });
    Ran {
        problem,
        built: started.elapsed(),
        executed: Duration::ZERO,
        projection: None,
    }
}

/// The cases of `tests/run/portable.txt`, by path without `.fl`.
fn portable(root: &Path) -> Vec<String> {
    listed_cases(root, "portable.txt")
}

/// The cases a list under `root` names, by path without `.fl`.
fn listed_cases(root: &Path, list: &str) -> Vec<String> {
    let text =
        fs::read_to_string(root.join(list)).unwrap_or_else(|_| panic!("readable tests/run/{list}"));
    let cases: Vec<String> = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    for case in &cases {
        assert!(
            root.join(format!("{case}.fl")).exists(),
            "tests/run/{list} lists `{case}`, which is no case"
        );
    }
    cases
}

/// The cases under `root` that `FLICK_E2E_ONLY` and `FLICK_E2E_SHARD`
/// select, sorted, and whether they are only a part of them.
fn selected(root: &Path) -> (Vec<PathBuf>, bool) {
    let mut paths = Vec::new();
    cases(root, &mut paths);
    if let Ok(only) = env::var("FLICK_E2E_ONLY") {
        paths.retain(|p| only.split(',').any(|o| p.to_string_lossy().contains(o)));
    }
    paths.sort();
    let part = shard(&mut paths) || env::var_os("FLICK_E2E_ONLY").is_some();
    assert!(!paths.is_empty(), "no cases under {}", root.display());
    (paths, part)
}

/// Keeps the shard `FLICK_E2E_SHARD` names, `k/n`: every n-th of the sorted
/// cases from the k-th on. Whether there is one.
fn shard(paths: &mut Vec<PathBuf>) -> bool {
    let Some(spec) = env::var("FLICK_E2E_SHARD").ok().filter(|s| !s.is_empty()) else {
        return false;
    };
    let (k, n) = spec
        .split_once('/')
        .and_then(|(k, n)| Some((k.parse::<usize>().ok()?, n.parse::<usize>().ok()?)))
        .filter(|&(k, n)| (1..=n).contains(&k))
        .unwrap_or_else(|| panic!("FLICK_E2E_SHARD is `k/n` with 1 <= k <= n, not `{spec}`"));
    let mut i = 0;
    paths.retain(|_| {
        i += 1;
        (i - 1) % n == k - 1
    });
    true
}

/// How many cases run at once.
fn jobs() -> usize {
    env::var("FLICK_E2E_JOBS")
        .ok()
        .and_then(|j| j.parse().ok())
        .filter(|&j| j > 0)
        .unwrap_or_else(|| thread::available_parallelism().map_or(1, usize::from))
}

/// Runs `f` on each of `items` on `jobs` threads; the results keep the
/// order of `items`.
fn parallel<T: Sync, R: Send>(items: &[T], jobs: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new(items.iter().map(|_| None).collect());
    thread::scope(|s| {
        for _ in 0..jobs.min(items.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else {
                        break;
                    };
                    let result = f(item);
                    results.lock().expect("no worker panicked")[i] = Some(result);
                }
            });
        }
    });
    results
        .into_inner()
        .expect("no worker panicked")
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

/// One pass of `f` over `cases` on `jobs` threads: what `f` reports of
/// each case, in order, and how long the pass and each case took.
fn pass<R: Send>(
    emit: &'static str,
    profile: &'static str,
    cases: &[PathBuf],
    jobs: usize,
    f: impl Fn(&PathBuf) -> (R, Timing) + Sync,
) -> (Vec<R>, Pass) {
    let started = Instant::now();
    let (results, cases) = parallel(cases, jobs, f).into_iter().unzip();
    let wall = started.elapsed();
    let timed = Pass {
        emit,
        profile,
        wall,
        cases,
    };
    (results, timed)
}

fn assert_go() {
    assert!(
        Command::new("go")
            .arg("version")
            .output()
            .is_ok_and(|o| o.status.success()),
        "the go target needs Go: `go` is not on PATH (Go >= 1.23)"
    );
}

fn installed_js_packages(repo: &Path, go_only: bool) {
    assert!(
        go_only || repo.join("node_modules/@flick-lang/runtime").exists(),
        "runtime is not installed: run `pnpm install --frozen-lockfile` in the repository root"
    );
    assert!(
        go_only || repo.join("node_modules/@flick-lang/effect-ts").exists(),
        "Effect adapter is not installed: run `pnpm install --frozen-lockfile` in the repository root"
    );
}

fn e2e_output_root(part: bool) -> PathBuf {
    // A filtered run must not typecheck TypeScript left by another run.
    let name = if part {
        format!("e2e-part-{}", process::id())
    } else {
        "e2e".to_owned()
    };
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

#[test]
#[ignore = "needs Node and `pnpm install`; run `just e2e`"]
fn run_cases() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let go_only = env::var_os("FLICK_E2E_GO_ONLY").is_some();
    let ts_runs = env::var_os("FLICK_E2E_TS_BUILD_ONLY").is_none();
    installed_js_packages(&repo, go_only);
    let root = repo.join("tests/run");
    let (paths, part) = selected(&root);
    let gaps = gaps::load(&root);
    let is_gap = |target: &str, case: &Path| gaps::listed(&gaps, target, &root, case);

    // Under target/, so the runtime and Effect adapter resolve from the repository root
    // node_modules exactly as it would in a project.
    let out_root = e2e_output_root(part);
    let mut report = Vec::new();
    let mut passes = Vec::new();
    let jobs = jobs();
    // what the JS debug run of each case projects its trace to
    let mut projections: HashMap<PathBuf, String> = HashMap::new();
    // release must agree with the debug reference: same stdout, stderr and exit
    let node_runs = [
        ("js", "debug", true),
        ("ts", "debug", ts_runs),
        ("js", "release", true),
        ("ts", "release", ts_runs),
    ];
    for &(emit, profile, runs) in if go_only { &[][..] } else { &node_runs } {
        let (results, timed) = pass(emit, profile, &paths, jobs, |case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root
                .join(profile)
                .join(emit)
                .join(rel.with_extension(""));
            let ran = if runs {
                run_case(case, rel, &out_dir, emit, profile)
            } else {
                build_case(case, &out_dir, emit, profile, &[])
            };
            let timing = ran.timing(rel);
            let gap = is_gap("js", case);
            // a gap fails when it runs, so a build alone says nothing of it
            let problem = gaps::verdict(gap, ran.problem)
                .filter(|_| runs || !gap)
                .map(|p| format!("{} ({emit}, {profile}):\n{p}", rel.display()));
            ((problem, ran.projection.filter(|_| !gap)), timing)
        });
        passes.push(timed);
        for (case, (problem, projection)) in paths.iter().zip(results) {
            report.extend(problem);
            if let (("js", "debug"), Some(p)) = ((emit, profile), projection) {
                projections.insert(case.clone(), p);
            }
        }
    }
    let portable = portable(&root);
    let (go_cases, skipped): (Vec<PathBuf>, Vec<PathBuf>) = paths.iter().cloned().partition(|p| {
        let rel = p.strip_prefix(&root).expect("case under root");
        portable.contains(&rel.with_extension("").to_string_lossy().replace('\\', "/"))
    });
    if !go_cases.is_empty() {
        assert_go();
    }
    for profile in ["debug", "release"] {
        let (problems, timed) = pass("go", profile, &go_cases, jobs, |case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root
                .join(profile)
                .join("go")
                .join(rel.with_extension(""));
            let ran = run_case(case, rel, &out_dir, "go", profile);
            let timing = ran.timing(rel);
            let differs = match (projections.get(case), &ran.projection) {
                (Some(js), Some(go)) => trace::differ(js, go),
                _ => None,
            };
            let problems: Vec<String> = ran.problem.into_iter().chain(differs).collect();
            let problem = gaps::verdict(
                is_gap("go", case),
                (!problems.is_empty()).then(|| problems.join("\n")),
            )
            .map(|p| format!("{} (go, {profile}):\n{p}", rel.display()));
            (problem, timing)
        });
        report.extend(problems.into_iter().flatten());
        passes.push(timed);
    }
    for case in &skipped {
        let rel = case.strip_prefix(&root).expect("case under root");
        println!("skip go: {} (not marked portable)", rel.display());
    }
    println!(
        "go target: {} portable cases, {} skipped",
        go_cases.len(),
        skipped.len()
    );
    times::report(&out_root, &passes, jobs);
    if go_only {
        assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
        return;
    }
    // a part of only gaps of the JS target may have written no TypeScript
    if paths.iter().any(|case| !is_gap("js", case)) {
        report.extend(typecheck(&repo, &out_root.join("debug/ts")));
        report.extend(typecheck(&repo, &out_root.join("release/ts")));
    }
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
    // the sizes and build times of the whole corpus only compare in one run
    if !part {
        // the sum of the builds: with the same jobs in both profiles it compares them
        times::metrics(&out_root, passes[0].built(), passes[2].built());
    }
}

include!("e2e/parameterized_modules.rs");
include!("e2e/module_expressions.rs");
include!("e2e/module_lifetimes.rs");
include!("e2e/modules.rs");
include!("e2e/build.rs");
