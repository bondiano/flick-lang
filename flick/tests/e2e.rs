//! E2E harness: builds each `tests/run/**/<name>.fl` with `flick build`, runs
//! the runner in Node with source maps and compares stdout, stderr and exit
//! code with `<name>.stdout` (required), `<name>.stderr` (default empty) or
//! `<name>.stderr.contains` (lines stderr must contain), and `<name>.exit`
//! (default `0`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Builds and runs one case, as JS or as TypeScript; returns the mismatch
/// report, if any.
fn run_case(case: &Path, out_dir: &Path, emit: &str) -> Option<String> {
    let _ = fs::remove_dir_all(out_dir);
    let build = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .arg(case)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--emit")
        .arg(emit)
        .output()
        .expect("flick runs");
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
    for emit in ["js", "ts"] {
        for case in &paths {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root.join(emit).join(rel.with_extension(""));
            if let Some(p) = run_case(case, &out_dir, emit) {
                report.push(format!("{} ({emit}):\n{p}", rel.display()));
            }
        }
    }
    report.extend(typecheck(&repo, &out_root.join("ts")));
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
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
