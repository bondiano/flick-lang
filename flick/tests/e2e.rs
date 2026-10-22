//! E2E harness: compiles each `tests/run/**/<name>.fl` to ESM, runs it in Node
//! and compares stdout, stderr and exit code with required `<name>.stdout`, optional
//! `<name>.stderr` (default empty) and `<name>.exit` (default `0`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use flick_core::source::SourceMap;

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

fn expected(case: &Path, ext: &str, default: &str) -> String {
    fs::read_to_string(case.with_extension(ext)).unwrap_or_else(|_| default.to_owned())
}

fn compare(what: &str, expected: &str, actual: &str) -> Option<String> {
    (expected != actual)
        .then(|| format!("  {what}:\n    expected: {expected:?}\n    actual:   {actual:?}"))
}

/// Compiles and runs one case; returns the mismatch report, if any.
fn run_case(case: &Path, out_dir: &Path) -> Option<String> {
    let mut sources = SourceMap::default();
    let file = sources.add(
        case.display().to_string(),
        fs::read_to_string(case).expect("UTF-8 case"),
    );
    // shortcut: calls the bootstrap codegen directly; switch to `flick build` in M7
    let js = match flick_core::bootstrap::compile(&sources, file) {
        Ok(js) => js,
        Err(diagnostics) => {
            let rendered =
                flick_core::diagnostics::render::render_human(&sources, &diagnostics, false);
            return Some(format!("  compile failed:\n{rendered}"));
        }
    };
    let module = out_dir.join(case.with_extension("mjs").file_name().expect("file name"));
    fs::write(&module, js).expect("write compiled module");
    let output = Command::new("node")
        .arg(&module)
        .output()
        .expect("`node` is on PATH (Node.js >= 24.18.0)");

    let problems: Vec<_> = [
        compare(
            "stdout",
            &fs::read_to_string(case.with_extension("stdout")).expect("`<name>.stdout` exists"),
            &String::from_utf8_lossy(&output.stdout),
        ),
        compare(
            "stderr",
            &expected(case, "stderr", ""),
            &String::from_utf8_lossy(&output.stderr),
        ),
        compare(
            "exit",
            expected(case, "exit", "0").trim(),
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
    let report: Vec<_> = paths
        .iter()
        .filter_map(|case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root.join(rel.parent().expect("has parent"));
            fs::create_dir_all(&out_dir).expect("create output dir");
            run_case(case, &out_dir).map(|p| format!("{}:\n{p}", rel.display()))
        })
        .collect();
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
}
