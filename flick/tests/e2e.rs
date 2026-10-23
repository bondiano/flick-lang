//! E2E harness: builds each `tests/run/**/<name>.fl` with `flick build`, runs
//! the runner in Node with source maps and compares stdout, stderr and exit
//! code with `<name>.stdout` (required), `<name>.stderr` (default empty) or
//! `<name>.stderr.contains` (lines stderr must contain), and `<name>.exit`
//! (default `0`).

use std::cell::Cell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use std::{env, fs};

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
/// the mismatch report, if any.
fn run_case(case: &Path, out_dir: &Path, emit: &str, profile: &str) -> Option<String> {
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
    BUILD_TIME.with(|t| t.set(t.get() + started.elapsed()));
    if !build.status.success() {
        return Some(format!(
            "  build failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        ));
    }
    let output = Command::new("node")
        .arg("--enable-source-maps")
        .arg(out_dir.join(format!("main.{emit}")))
        .output()
        .expect("`node` is on PATH (Node.js >= 24.18.0)");
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
    (!problems.is_empty()).then(|| problems.join("\n"))
}

#[test]
#[ignore = "needs Node and `pnpm install`; run `just e2e`"]
fn run_cases() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    assert!(
        repo.join("node_modules/@flick-lang/runtime").exists(),
        "runtime is not installed: run `pnpm install --frozen-lockfile` in the repository root"
    );
    let root = repo.join("tests/run");
    let mut paths = Vec::new();
    cases(&root, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no cases under {}", root.display());

    // Under target/, so `@flick-lang/runtime` resolves from the repository root
    // node_modules exactly as it would in a project.
    let out_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e");
    let mut report = Vec::new();
    let mut times = Vec::new();
    // release must agree with the debug reference: same stdout, stderr and exit
    for (emit, profile) in [("js", "debug"), ("ts", "debug"), ("js", "release")] {
        BUILD_TIME.with(|t| t.set(Duration::ZERO));
        for case in &paths {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root
                .join(profile)
                .join(emit)
                .join(rel.with_extension(""));
            if let Some(p) = run_case(case, &out_dir, emit, profile) {
                report.push(format!("{} ({emit}, {profile}):\n{p}", rel.display()));
            }
        }
        times.push(BUILD_TIME.with(Cell::get));
    }
    report.extend(typecheck(&repo, &out_root.join("debug/ts")));
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
    metrics(&out_root, times[0], times[2]);
}

thread_local! {
    static BUILD_TIME: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// How much longer the release builds of the corpus may take than the debug
/// ones, until M19 fixes the budgets.
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
        } else if path.extension().is_some_and(|e| e == "js") && !path.ends_with("main.js") {
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
    let mut report = Vec::new();
    for case in paths {
        let rel = case.strip_prefix(&root).expect("case under root");
        let out_dir = out_root.join(rel.with_extension(""));
        let _ = fs::remove_dir_all(&out_dir);
        let build = Command::new(env!("CARGO_BIN_EXE_flick"))
            .arg("build")
            .arg(&case)
            .arg("--out-dir")
            .arg(&out_dir)
            .output()
            .expect("flick runs");
        let stderr = String::from_utf8_lossy(&build.stderr);
        // blocks compiled by later phases are reported, not miscompiled
        if !build.status.success() && !stderr.contains("FLICK-P012") {
            report.push(format!("{}: {stderr}", rel.display()));
            continue;
        }
        let mut files = Vec::new();
        scripts(&out_dir, &mut files);
        for file in files {
            let check = Command::new("node")
                .arg("--check")
                .arg(&file)
                .output()
                .expect("node");
            if !check.status.success() {
                report.push(format!(
                    "{}: {}",
                    file.display(),
                    String::from_utf8_lossy(&check.stderr)
                ));
            }
        }
    }
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
