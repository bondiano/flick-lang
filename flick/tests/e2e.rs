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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use flick_core::build::go::executable;
use flick_core::build::runner_path;
use flick_core::project::config::MAIN_ENTRY;

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

/// Builds and runs one case, as JS or as TypeScript, in a profile; returns
/// the mismatch report, if any, and how long the build took.
fn run_case(case: &Path, out_dir: &Path, emit: &str, profile: &str) -> (Option<String>, Duration) {
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
        .output()
        .expect("flick runs");
    let build_time = started.elapsed();
    if !build.status.success() {
        let report = format!(
            "  build failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        return (Some(report), build_time);
    }
    let output = if emit == "go" {
        Command::new(out_dir.join(executable(MAIN_ENTRY)))
            .output()
            .expect("the built executable runs")
    } else {
        Command::new("node")
            .arg("--enable-source-maps")
            .arg(out_dir.join(runner_path(emit == "ts")))
            .output()
            .expect("`node` is on PATH (Node.js >= 24.18.0)")
    };
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
    ]
    .into_iter()
    .flatten()
    .collect();
    (
        (!problems.is_empty()).then(|| problems.join("\n")),
        build_time,
    )
}

/// The cases of `tests/run/portable.txt`, by path without `.fl`.
fn portable(root: &Path) -> Vec<String> {
    let text =
        fs::read_to_string(root.join("portable.txt")).expect("readable tests/run/portable.txt");
    let cases: Vec<String> = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    for case in &cases {
        assert!(
            root.join(format!("{case}.fl")).exists(),
            "tests/run/portable.txt lists `{case}`, which is no case"
        );
    }
    cases
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

#[test]
#[ignore = "needs Node and `pnpm install`; run `just e2e`"]
fn run_cases() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let go_only = env::var_os("FLICK_E2E_GO_ONLY").is_some();
    assert!(
        go_only || repo.join("node_modules/@flick-lang/runtime").exists(),
        "runtime is not installed: run `pnpm install --frozen-lockfile` in the repository root"
    );
    let root = repo.join("tests/run");
    let mut paths = Vec::new();
    cases(&root, &mut paths);
    if let Ok(only) = env::var("FLICK_E2E_ONLY") {
        paths.retain(|p| only.split(',').any(|o| p.to_string_lossy().contains(o)));
    }
    paths.sort();
    assert!(!paths.is_empty(), "no cases under {}", root.display());

    // Under target/, so `@flick-lang/runtime` resolves from the repository root
    // node_modules exactly as it would in a project.
    let out_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e");
    let mut report = Vec::new();
    let mut times = Vec::new();
    let jobs = jobs();
    // release must agree with the debug reference: same stdout, stderr and exit
    let node_runs = [
        ("js", "debug"),
        ("ts", "debug"),
        ("js", "release"),
        ("ts", "release"),
    ];
    for &(emit, profile) in if go_only { &[][..] } else { &node_runs } {
        let results = parallel(&paths, jobs, |case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root
                .join(profile)
                .join(emit)
                .join(rel.with_extension(""));
            let (problem, built) = run_case(case, &out_dir, emit, profile);
            let problem = problem.map(|p| format!("{} ({emit}, {profile}):\n{p}", rel.display()));
            (problem, built)
        });
        // the sum of the builds: with the same jobs in both profiles it compares them
        times.push(results.iter().map(|(_, built)| *built).sum::<Duration>());
        report.extend(results.into_iter().filter_map(|(problem, _)| problem));
    }
    let portable = portable(&root);
    let (go_cases, skipped): (Vec<PathBuf>, Vec<PathBuf>) = paths.iter().cloned().partition(|p| {
        let rel = p.strip_prefix(&root).expect("case under root");
        portable.contains(&rel.with_extension("").to_string_lossy().replace('\\', "/"))
    });
    if !go_cases.is_empty() {
        assert!(
            Command::new("go")
                .arg("version")
                .output()
                .is_ok_and(|o| o.status.success()),
            "the go target needs Go: `go` is not on PATH (Go >= 1.23)"
        );
    }
    for profile in ["debug", "release"] {
        let results = parallel(&go_cases, jobs, |case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root
                .join(profile)
                .join("go")
                .join(rel.with_extension(""));
            let (problem, _) = run_case(case, &out_dir, "go", profile);
            problem.map(|p| format!("{} (go, {profile}):\n{p}", rel.display()))
        });
        report.extend(results.into_iter().flatten());
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
    if go_only {
        assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
        return;
    }
    report.extend(typecheck(&repo, &out_root.join("debug/ts")));
    report.extend(typecheck(&repo, &out_root.join("release/ts")));
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
    metrics(&out_root, times[0], times[2]);
}

#[test]
#[ignore = "needs Node and `pnpm install`; run `just e2e`"]
fn compiled_ui_core_imports_without_a_dom_renderer() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui_core_import");
    let build = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .arg(repo.join("tests/check/pass/ui/views.fl"))
        .arg("--out-dir")
        .arg(&dir)
        .args(["--emit", "js"])
        .output()
        .expect("flick runs");
    assert!(build.status.success(), "{build:?}");
    let imported = Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            r#"
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { pathToFileURL } from "node:url";
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === "react-dom" || specifier.startsWith("react-dom/")) {
      throw new Error("a compiled Ui.mount_with consumer loaded react-dom");
    }
    return next(specifier, context);
  }
});
assert.equal(typeof document, "undefined");
assert.equal(typeof window, "undefined");
const app = await import(pathToFileURL(process.argv[1]).href);
assert.equal(typeof app.run, "function");
assert.equal(app.app.kind, "component");
"#,
        ])
        .arg(dir.join("App/Views.js"))
        .output()
        .expect("Node is installed");
    assert!(imported.status.success(), "{imported:?}");
}

/// How much longer the release builds of the corpus may take than the debug
/// ones.
const RELEASE_TIME_BUDGET: f64 = 2.0;

/// The JS size and build time of the corpus in both profiles: release must
/// be smaller and within its time budget. CI shows the numbers in the job
/// summary.
fn metrics(out_root: &Path, debug_time: Duration, release_time: Duration) {
    let size = |profile: &str| js_size(&out_root.join(profile).join("js"));
    let (debug, release) = (size("debug"), size("release"));
    let line = format!(
        "e2e corpus: JS {debug} bytes in debug, {release} in release ({:+.1}%); build {:.2}s in debug, {:.2}s in release",
        percent(release, debug),
        debug_time.as_secs_f64(),
        release_time.as_secs_f64(),
    );
    println!("{line}");
    if let Ok(summary) = env::var("GITHUB_STEP_SUMMARY") {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)
            .expect("summary");
        writeln!(file, "{line}").expect("write summary");
    }
    assert!(release < debug, "{line}");
    assert!(
        release_time.as_secs_f64() <= debug_time.as_secs_f64() * RELEASE_TIME_BUDGET,
        "{line}"
    );
}

/// How much `new` differs from `old`, in percent.
fn percent(new: u64, old: u64) -> f64 {
    let (new, old) = (
        u32::try_from(new).unwrap_or(u32::MAX),
        u32::try_from(old).unwrap_or(u32::MAX),
    );
    (f64::from(new) / f64::from(old) - 1.0) * 100.0
}

/// Bytes of the compiled modules under `dir`, the built-in ones left out.
fn js_size(dir: &Path) -> u64 {
    let mut total = 0;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() && !path.ends_with("_flick") {
            total += js_size(&path);
        } else if path.extension().is_some_and(|e| e == "js") && !path.ends_with(runner_path(false))
        {
            total += entry.metadata().map_or(0, |m| m.len());
        }
    }
    total
}

/// `tsc --noEmit` over everything `--emit ts` wrote under `dir`.
fn typecheck(repo: &Path, dir: &Path) -> Option<String> {
    let config = r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "allowImportingTsExtensions": true,
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "target": "es2022",
    "skipLibCheck": true,
    "types": []
  },
  "include": ["**/*.ts"]
}
"#;
    fs::write(dir.join("tsconfig.json"), config).expect("write tsconfig");
    let tsc = Command::new(repo.join("node_modules/.bin/tsc"))
        .arg("-p")
        .arg(dir)
        .output()
        .expect("`tsc` is installed: run `pnpm install`");
    (!tsc.status.success())
        .then(|| format!("tsc --noEmit:\n{}", String::from_utf8_lossy(&tsc.stdout)))
}

/// Every module the checker accepts compiles to JS that Node can parse.
#[test]
#[ignore = "needs Node; run `just e2e`"]
fn passing_fixtures_compile_to_valid_js() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = repo.join("tests/check/pass");
    let mut paths = Vec::new();
    cases(&root, &mut paths);
    let out_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fixtures");
    let report: Vec<String> = parallel(&paths, jobs(), |case| {
        let rel = case.strip_prefix(&root).expect("case under root");
        let out_dir = out_root.join(rel.with_extension(""));
        let _ = fs::remove_dir_all(&out_dir);
        let build = Command::new(env!("CARGO_BIN_EXE_flick"))
            .arg("build")
            .arg(case)
            .arg("--out-dir")
            .arg(&out_dir)
            .output()
            .expect("flick runs");
        let stderr = String::from_utf8_lossy(&build.stderr);
        // blocks compiled by later phases are reported, not miscompiled
        if !build.status.success() && !stderr.contains("FLICK-P012") {
            return vec![format!("{}: {stderr}", rel.display())];
        }
        let mut files = Vec::new();
        scripts(&out_dir, &mut files);
        files
            .iter()
            .filter_map(|file| {
                let check = Command::new("node")
                    .arg("--check")
                    .arg(file)
                    .output()
                    .expect("node");
                (!check.status.success()).then(|| {
                    format!(
                        "{}: {}",
                        file.display(),
                        String::from_utf8_lossy(&check.stderr)
                    )
                })
            })
            .collect()
    })
    .concat();
    assert!(report.is_empty(), "{}", report.join("\n"));
}

fn scripts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scripts(&path, out);
        } else if path.extension().is_some_and(|e| e == "js") {
            out.push(path);
        }
    }
}
